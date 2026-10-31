"""Writes an Android SDK package's package.xml from Google's repository manifest.

Usage: python scripts/android-package-xml.py <manifest.xml> <archive-sha1> <package.xml>

scripts/install-android-sdk.sh unpacks pinned archives itself, so nothing
writes the metadata sdkmanager would: AGP and avdmanager then miss packages
such as a platform with a minor API level or a 16 KB system image. This copies
the manifest entry whose archive has the pinned SHA-1, as sdkmanager does:
the remotePackage becomes a localPackage, without its channel, archives and
licence reference (ADR 0089).
"""

import re
import sys


def main() -> None:
    manifest, sha1, out = sys.argv[1:4]
    text = open(manifest, encoding="utf-8").read()
    root = re.search(r"<(\w[\w.-]*:)?[\w-]+ ([^>]*xmlns[^>]*)>", text)
    if root is None:
        sys.exit(f"{manifest}: no root element")
    namespaces = " ".join(re.findall(r'xmlns:[\w-]+="[^"]*"', root.group(2)))
    for package in re.findall(r"<remotePackage .*?</remotePackage>", text, re.S):
        if f'<checksum type="sha1">{sha1}</checksum>' not in package:
            continue
        local = re.sub(r"\s*<channelRef[^>]*/>", "", package)
        local = re.sub(r"\s*<archives>.*?</archives>", "", local, flags=re.S)
        local = re.sub(r"\s*<uses-license[^>]*/>", "", local)
        local = local.replace("<remotePackage ", '<localPackage obsolete="false" ', 1)
        local = local.replace("</remotePackage>", "</localPackage>")
        if 'xmlns:ns2="http://schemas.android.com/repository/android/common/02"' not in namespaces:
            namespaces = 'xmlns:ns2="http://schemas.android.com/repository/android/common/02" ' + namespaces
        with open(out, "w", encoding="utf-8", newline="\n") as file:
            file.write('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n')
            file.write(f"<ns2:repository {namespaces}>\n  {local}\n</ns2:repository>\n")
        return
    sys.exit(f"{manifest}: no package with an archive of SHA-1 {sha1}")


if __name__ == "__main__":
    main()
