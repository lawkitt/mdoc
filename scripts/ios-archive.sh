#!/usr/bin/env bash
# Usage: scripts/ios-archive.sh archive
#        scripts/ios-archive.sh export | validate | upload
# The iOS TestFlight build (ADR 0107); ios-release.yml runs it after the
# core is built (`just ios-core`), with the build number from the full
# history (MDOC_REQUIRE_FULL_HISTORY=1).
#   archive   Archives the Release configuration unsigned, then signs the
#             archived app and Restore extension ad hoc with their
#             entitlements ($(AppIdentifierPrefix) filled in), so no signing
#             certificate is needed on the runner.
#   export    Exports an App Store .ipa into target/ios/Export; Apple-managed
#             (cloud) signing replaces the ad hoc signatures with the team's
#             distribution certificate and App Store profiles, keeping those
#             entitlements.
#   validate  export, then Apple's validation of the .ipa (altool), with no
#             upload: the workflow's validate mode.
#   upload    Exports straight to App Store Connect. An upload Apple already
#             has (a rerun after a good upload) counts as done.
# export, validate and upload sign in with the App Store Connect key:
# ASC_KEY_PATH (scripts/ios-asc-key.sh), ASC_KEY_ID and ASC_ISSUER_ID.
# Without them export uses the account signed in to Xcode, for a check on a
# Mac. Bash 3.2.
source "$(dirname "$0")/lib.sh"
[ "$(uname -s)" = Darwin ] || die "iOS archives build only on macOS"

team=F33DLM8K53
project=apps/ios/mdoc.xcodeproj
archive=target/ios/Archive/mdoc.xcarchive
export_dir=target/ios/Export
app=$archive/Products/Applications/mdoc.app
usage="usage: scripts/ios-archive.sh archive | export | validate | upload"

# entitlements <file> <out>: the target's entitlements as the build fills
# them in; fails on any setting left unfilled.
entitlements() {
  sed "s/\$(AppIdentifierPrefix)/$team./g" "$1" >"$2"
  # shellcheck disable=SC2016 # a literal $( in the file
  ! grep -q '\$(' "$2" || die "$1 uses a build setting this script does not fill in: $(grep '\$(' "$2")"
  plutil -lint -s "$2" || die "$1 is not a valid plist"
}

do_archive() {
  local work
  rm -rf "$archive"
  xcodebuild -project "$project" -scheme mdoc -configuration Release -destination generic/platform=iOS \
    -archivePath "$archive" -derivedDataPath target/ios/ArchiveData -quiet archive \
    CODE_SIGNING_ALLOWED=NO
  [ -d "$app/PlugIns/RestoreExtension.appex" ] || die "the archive has no Restore extension"
  work=$(mktemp -d)
  entitlements apps/ios/Config/RestoreExtension.entitlements "$work/RestoreExtension.plist"
  entitlements apps/ios/Config/mdoc.entitlements "$work/mdoc.plist"
  # Inside out: frameworks, the extension, then the app around them.
  local framework
  for framework in "$app"/Frameworks/*.framework; do
    [ -d "$framework" ] && codesign --force --sign - "$framework"
  done
  codesign --force --sign - --entitlements "$work/RestoreExtension.plist" "$app/PlugIns/RestoreExtension.appex"
  codesign --force --sign - --entitlements "$work/mdoc.plist" "$app"
  rm -rf "$work"
  echo "archived build $(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$app/Info.plist") to $archive"
}

# options <destination>: the export options for an App Store build. Xcode
# must not renumber the build (ADR 0091's number is the commit count).
options() {
  cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>$1</string>
  <key>teamID</key><string>$team</string>
  <key>signingStyle</key><string>automatic</string>
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>uploadSymbols</key><true/>
</dict>
</plist>
EOF
}

# auth: xcodebuild's App Store Connect key arguments, one a line; none
# without a key.
auth() {
  if [ -n "${ASC_KEY_PATH:-}" ]; then
    : "${ASC_KEY_ID:?ASC_KEY_ID is not set}" "${ASC_ISSUER_ID:?ASC_ISSUER_ID is not set}"
    printf '%s\n' -authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" \
      -authenticationKeyIssuerID "$ASC_ISSUER_ID"
  fi
}

do_export() {
  local destination=$1 plist log status=0 args=()
  [ -d "$app" ] || die "no archive at $archive: run \`scripts/ios-archive.sh archive\` first"
  plist=$(mktemp)
  log=$(mktemp)
  options "$destination" >"$plist"
  while IFS= read -r line; do args+=("$line"); done < <(auth)
  rm -rf "$export_dir"
  xcodebuild -exportArchive -archivePath "$archive" -exportPath "$export_dir" -exportOptionsPlist "$plist" \
    -allowProvisioningUpdates ${args[@]+"${args[@]}"} 2>&1 | tee "$log" || status=$?
  if [ "$status" = 0 ] && ! grep -q 'EXPORT SUCCEEDED' "$log"; then status=1; fi
  if [ "$status" != 0 ] && [ "$destination" = upload ] &&
    grep -qiE 'redundant binary upload|bundle version must be higher|has already been (used|uploaded)' "$log"; then
    echo "App Store Connect already has this build; counted as uploaded"
    status=0
  fi
  rm -f "$plist" "$log"
  return "$status"
}

do_validate() {
  local ipa
  do_export export
  ipa=$export_dir/mdoc.ipa
  [ -f "$ipa" ] || die "the export made no $ipa"
  : "${ASC_KEY_PATH:?validate needs the App Store Connect key (ASC_KEY_PATH)}"
  xcrun altool --validate-app --file "$ipa" --type ios --api-key "$ASC_KEY_ID" \
    --api-issuer "$ASC_ISSUER_ID" --p8-file-path "$ASC_KEY_PATH"
}

case "${1:-}" in
  archive) do_archive ;;
  export) do_export export ;;
  validate) do_validate ;;
  upload) do_export upload ;;
  *) die "$usage" ;;
esac
