# R8 rules for the app's minified builds (the `minified` build type, ADR
# 0091). The core is Rust behind UniFFI's Kotlin bindings, which call it
# through JNA; R8 cannot see what either reaches by name at run time.
# scripts/android-check.sh opens the core in the minified app on the
# emulator, so a rule missing here fails the Android job.

# JNA: its native dispatch library (libjnidispatch.so) finds JNA's own
# classes, fields and methods by name. AWT is desktop-only.
-keep class com.sun.jna.** { *; }
-dontwarn java.awt.**

# The bindings: JNA reads each Structure's fields by the names in its
# @FieldOrder and calls each Callback's one method by reflection, and the
# library interface's functions are the core's C symbols by name.
-keep class uniffi.mdoc_ffi.** { *; }
