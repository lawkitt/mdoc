- **Macs test on CI's exact iOS Simulator runtime.** `scripts/tools.env`
  pins the runtime's build (iOS 26.2, 23C54) beside its version, which
  `scripts/ios-toolchain.sh` downloads and `just doctor` and
  `just mobile --ios` check; asking for version 26.2 alone fetched the
  23C52 release candidate (#654).
