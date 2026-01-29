# Air1-Monitor Cross Build Analysis

## Summary

The CI build using `cross` for x86_64-unknown-linux-gnu fails due to **two primary issues**:

### Issue 1: Missing System Dependencies ✅ FIXED
**Problem**: The cross container lacks `libdbus-1-dev` and `pkg-config`, required by the `libdbus-sys` crate.

**Solution**: Add pre-build commands to [Cross.toml](Cross.toml) to install required packages.

### Issue 2: aws-lc-sys GCC Compiler Bug ⚠️ REQUIRES FURTHER INVESTIGATION  
**Problem**: The GCC compiler in the cross base image (Ubuntu Focal) has a known memcmp bug (https://gcc.gnu.org/bugzilla/show_bug.cgi?id=95189). The `aws-lc-sys` crate detects this and fails the build.

The rust dependency chain is:
- `eframe` (GUI) → `wgpu` → likely uses SSL
- `rumqttc` (MQTT) → `rustls` (TLS) → `aws-lc-sys` (crypto provider)

**Attempted Solutions**:
1. ✅ **libdbus-sys fix**: Added pre-build dependencies (WORKING)
2. ⚠️ **Zig CC (partial)**: Enabled in Cross.toml but aws-lc-sys still detects old GCC
3. ❌ **AWS_LC_SYS_NO_ASM**: Only works for debug builds, not release
4. ❌ **GCC upgrade**: gcc-11 not available in focal repos
5. ❌ **Latest cross image**: Has GLIBC compatibility issues

## Current Cross.toml Configuration

```toml
[target.x86_64-unknown-linux-gnu]
pre-build = [
    "apt-get update && apt-get install -y pkg-config libdbus-1-dev libssl-dev"
]
zig = "2.17"

[target.x86_64-pc-windows-gnu]
pre-build = [
    "apt-get update && apt-get install -y pkg-config libdbus-1-dev libssl-dev"
]

[target.aarch64-unknown-linux-gnu]
pre-build = [
    "apt-get update && apt-get install -y pkg-config libdbus-1-dev:arm64 libssl-dev:arm64"
]
zig = "2.17"

[target.armv7-unknown-linux-gnueabihf]
pre-build = [
    "apt-get update && apt-get install -y pkg-config libdbus-1-dev:armhf libssl-dev:armhf"
]
zig = "2.17"
```

## Recommended Next Steps

### Option A: Use native Linux containers for CI (Best for Ubuntu Focal)
Instead of cross, use container engines that provide the full target environment. The GitLab CI is already set up with Docker.

### Option B: Switch rustls crypto provider
Replace aws-lc-sys with Ring (already partially implemented):
```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
```
But this doesn't work because rumqttc forces aws-lc-sys dependency.

### Option C: Downgrade aws-lc-sys to older version
Check if aws-lc-sys < 0.37.0 doesn't have the GCC bug detection, but this is not a long-term solution.

### Option D: Use musl targets instead
Switch to `x86_64-unknown-linux-musl` which avoids GLIBC compatibility issues:
```bash
cross build --target x86_64-unknown-linux-musl --release
```

### Option E: Full Zig CC integration  
Ensure Zig CC is properly configured to replace GCC entirely. May need:
- Cross version ≥ 0.2.4
- Zig binary installation in cross container
- Proper CC/CXX environment variables

## Files Modified

1. **[Cross.toml](Cross.toml)** - NEW: Added pre-build dependencies and Zig CC configuration
2. **[.cargo/config.toml](.cargo/config.toml)** - Added (currently empty but available for future configuration)
3. **[Cargo.toml](Cargo.toml)** - No changes needed (rustls already uses ring)

## Error Messages Reference

### libdbus-sys Error (NOW FIXED)
```
The system library `dbus-1` required by crate `libdbus-sys` was not found.
The file `dbus-1.pc` needs to be installed and the PKG_CONFIG_PATH environment variable must contain its parent directory.
```

### aws-lc-sys GCC Bug Error
```
### COMPILER BUG DETECTED ###
Your compiler (cc) is not supported due to a memcmp related bug reported in https://gcc.gnu.org/bugzilla/show_bug.cgi?id=95189
```

## Resources
- Cross.rs Wiki: https://github.com/cross-rs/cross/wiki/FAQ
- Zig CC Support: https://github.com/cross-rs/cross/wiki/Configuration#target-specific
- aws-lc-sys Issue: https://gcc.gnu.org/bugzilla/show_bug.cgi?id=95189
