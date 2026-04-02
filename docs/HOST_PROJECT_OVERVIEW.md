# ShopRemote2 Host-Only Build Project Overview

**Project Status**: Planning Phase
**Date**: April 2, 2026
**Base Version**: ShopRemote2 2.0.1 (RustDesk 1.4.6 fork)

---

## Executive Summary

This document outlines the architecture and strategy for creating a **Host-Only variant** of ShopRemote2 - a stripped-down version that functions exclusively as a remote desktop server (like TeamViewer Host). The host-only build will accept incoming remote connections but will **not** support outgoing remote control functionality.

---

## Current Architecture

### High-Level Components

The current ShopRemote2 application is a unified binary with both **Host** (server) and **Controller** (client) functionality:

1. **Rust Backend** (`src/`) - Core remote desktop protocol implementation
2. **Flutter Frontend** (`flutter/lib/`) - Cross-platform UI (desktop & mobile)
3. **Shared Libraries** (`libs/`) - Video codec, networking, file transfer
4. **Configuration System** - Centralized config management

### Host vs Controller Functional Split

#### **HOST-SIDE (Server/Incoming) Functionality**
The "incoming only" mode enabled by `is_incoming_only()` includes:

- **Server Core** (`src/server.rs`)
  - TCP/UDP listening socket setup
  - Client connection management
  - Service initialization (audio, video, clipboard, input, display)
  - Connection state tracking and session lifecycle

- **Server Services** (`src/server/*.rs`)
  - `video_service.rs` - Screen capture and encoding (Windows, macOS, Linux)
  - `audio_service.rs` - Microphone input/output
  - `clipboard_service.rs` - Clipboard sync (send-only in server mode)
  - `input_service.rs` - Receive mouse/keyboard input from clients
  - `display_service.rs` - Monitor layout and cursor position tracking
  - `terminal_service.rs` (Linux/Windows) - Shell/terminal access
  - `portable_service.rs` (Windows) - Portable executable management
  - `printer_service.rs` (Windows) - Printer device handling

- **Flutter UI** - Host/Server Pages
  - `flutter/lib/desktop/pages/server_page.dart` - Connection Manager UI
  - `flutter/lib/desktop/pages/desktop_home_page.dart` - Displays ID/password for incoming connections
  - `flutter/lib/mobile/pages/server_page.dart` - Mobile server interface

- **Platform Services** (`src/platform/`)
  - Windows: Virtual display, service setup, privileged process management
  - macOS: Screen capture permissions, Dock integration
  - Linux: D-Bus integration, X11/Wayland support, PAM authentication

- **Network & Authentication**
  - Rendezvous server integration for device registration
  - ID generation and management
  - Password-based or token-based authentication
  - Encryption (sodiumoxide box_)

#### **CONTROLLER-SIDE (Client/Outgoing) Functionality**
The "outgoing only" mode enabled by `is_outgoing_only()` includes:

- **Client Core** (`src/client.rs`)
  - Initiates outgoing connections to remote machines
  - Handles remote device discovery and connection state
  - Decodes incoming video/audio streams
  - Sends local input (mouse/keyboard) to remote

- **Client Modules** (`src/client/*.rs`)
  - `io_loop.rs` - Message handling and event loop
  - `file_trait.rs` - Remote file transfer implementation
  - `helper.rs` - Connection utilities
  - `screenshot.rs` - Local screenshot capture (not used in normal mode)

- **Flutter UI** - Client/Remote Control Pages
  - `flutter/lib/desktop/pages/remote_page.dart` - List of remote devices to connect to
  - `flutter/lib/desktop/pages/remote_tab_page.dart` - Tab management for multiple connections
  - `flutter/lib/desktop/screen/desktop_remote_screen.dart` - Remote desktop viewer
  - `flutter/lib/desktop/pages/file_manager_page.dart` - Remote file transfer UI
  - `flutter/lib/desktop/pages/terminal_page.dart` - Remote terminal access
  - `flutter/lib/desktop/pages/port_forward_page.dart` - TCP tunnel setup
  - `flutter/lib/mobile/pages/remote_page.dart` - Mobile remote control UI

- **Port Forwarding** (`src/port_forward.rs`)
  - TCP tunnel creation from client to remote services
  - Used for accessing remote ports (SSH, web services, etc.)

#### **SHARED Functionality (Both Modes)**
- **Rendezvous & NAT Traversal** (`src/rendezvous_mediator.rs`)
- **Encryption & Authentication** (`src/auth_2fa.rs`)
- **Clipboard Management** (`src/clipboard.rs`)
- **Config Management** (`libs/hbb_common/src/config.rs`)
- **Video/Audio Codec** (`libs/hbb_common/src/video_frame.rs`)
- **Message Protocol** (protobuf definitions)

---

## How Host/Controller Modes Are Currently Managed

### Configuration-Based Mode Selection

The application **already has a built-in mechanism** for conditional host/controller functionality:

**File**: `libs/hbb_common/src/config.rs` (lines 2491-2506)
```rust
pub fn is_incoming_only() -> bool {
    HARD_SETTINGS.read().unwrap()
        .get("conn-type")
        .map_or(false, |x| x == ("incoming"))
}

pub fn is_outgoing_only() -> bool {
    HARD_SETTINGS.read().unwrap()
        .get("conn-type")
        .map_or(false, |x| x == ("outgoing"))
}
```

The `conn-type` setting is populated at startup from configuration files (hard settings) and controls:
- UI visibility (which pages are shown)
- Feature availability (some services disabled based on mode)
- Tray menu options
- Window resizing and UI layout

**Current Usage in Code**:
- Flutter UI conditionally renders based on `bind.isIncomingOnly()` / `bind.isOutgoingOnly()`
- Rust code checks `config::is_incoming_only()` / `config::is_outgoing_only()` to skip initialization
- Platform-specific code (Windows, macOS, Linux) modifies feature availability

### Limitation: Mode is Conditional, Not Compile-Time

Currently, the mode is **runtime-configurable** but **not compile-time exclusive**. A "normal" build includes both code paths and simply disables UI/features. Creating a host-only build requires:
1. Removing unnecessary UI pages
2. Stripping unused client code
3. Optimizing binary size
4. Optional: Using compile-time feature flags to exclude dead code entirely

---

## File Structure Overview

```
shopremote2_src/
├── src/
│   ├── server.rs                    # Server core
│   ├── server/
│   │   ├── connection.rs            # Client connection handling (core)
│   │   ├── video_service.rs         # Screen capture
│   │   ├── audio_service.rs         # Audio I/O
│   │   ├── input_service.rs         # Keyboard/mouse input handling
│   │   ├── clipboard_service.rs     # Clipboard
│   │   ├── display_service.rs       # Display/cursor tracking
│   │   ├── terminal_service.rs      # Shell access (not host-only)
│   │   ├── portable_service.rs      # Windows specific
│   │   └── printer_service.rs       # Windows specific
│   ├── client.rs                    # Client core (REMOVE for host-only)
│   ├── client/
│   │   ├── io_loop.rs               # (REMOVE for host-only)
│   │   ├── file_trait.rs            # (REMOVE for host-only)
│   │   └── screenshot.rs            # (REMOVE for host-only)
│   ├── flutter.rs                   # FFI bridge to Flutter
│   ├── flutter_ffi.rs               # FFI bindings (will need host-only variants)
│   ├── ui_interface.rs              # UI event handlers (mostly server)
│   ├── ui_session_interface.rs      # Session handling (mixed)
│   ├── ui_cm_interface.rs           # Connection manager UI (host)
│   ├── port_forward.rs              # TCP tunneling (REMOVE for host-only)
│   ├── rendezvous_mediator.rs       # Server registration (KEEP)
│   ├── platform/                    # Platform-specific code
│   │   ├── windows.rs               # Some host, some client features
│   │   ├── macos.rs                 # Mixed
│   │   └── linux.rs                 # Mixed
│   └── plugin/                      # Plugin system (OPTIONAL)
│
├── flutter/lib/
│   ├── main.dart                    # Entry point with desktop type routing
│   ├── desktop/
│   │   ├── pages/
│   │   │   ├── server_page.dart     # KEEP - Connection Manager
│   │   │   ├── desktop_home_page.dart # PARTIAL - Only host section
│   │   │   ├── remote_page.dart     # REMOVE - Client only
│   │   │   ├── remote_tab_page.dart # REMOVE - Client only
│   │   │   ├── connection_page.dart # PARTIAL - Host only
│   │   │   ├── file_manager_page.dart # REMOVE - Client file transfer
│   │   │   ├── terminal_page.dart   # REMOVE - Client terminal
│   │   │   ├── port_forward_page.dart # REMOVE - Client feature
│   │   │   └── desktop_setting_page.dart # PARTIAL - Host-only settings
│   │   └── widgets/                 # Mostly shared
│   ├── mobile/
│   │   └── pages/
│   │       ├── server_page.dart     # KEEP - Mobile host
│   │       ├── home_page.dart       # MODIFY - Host only
│   │       └── remote_page.dart     # REMOVE - Client only
│   ├── models/
│   │   ├── server_model.dart        # KEEP - Host state
│   │   ├── platform_model.dart      # KEEP - Shared
│   │   └── ...                      # Various model classes (PARTIAL)
│   └── common/                      # Shared widgets & utilities
│
├── libs/
│   ├── hbb_common/                  # Core library (KEEP all)
│   ├── scrap/                       # Video codec (KEEP all)
│   ├── enigo/                       # Input simulation (REMOVE for host-only)
│   ├── clipboard/                   # (KEEP all)
│   ├── virtual_display/             # Windows specific (KEEP)
│   └── remote_printer/              # Windows specific (KEEP)
│
├── Cargo.toml                        # Dependencies & features
├── build.py                          # Build script
└── build.rs                          # Rust build script
```

---

## Rust Feature Flag Strategy

Current Cargo.toml includes feature flags:
```
[features]
flutter = ["flutter_rust_bridge"]
hwcodec = ["scrap/hwcodec"]
vram = ["scrap/vram"]
... (others)
```

### Proposed Additions for Host-Only Build

Add new feature flags to `Cargo.toml`:

```rust
[features]
flutter = ["flutter_rust_bridge"]
host-only = []           # New: excludes client code
controller-only = []     # New: excludes server code (future use)
default = ["use_dasp"]
```

Then conditionally compile code:
```rust
#[cfg(not(feature = "host-only"))]
pub mod client;

#[cfg(feature = "host-only")]
pub mod client {
    // Empty stubs if needed
}
```

---

## Build Strategy: Recommended Approach

### Option A: Separate Flutter Flavor (Recommended)

Create distinct build targets using Flutter flavors while sharing Rust backend:

**Flutter Flavor Approach**:
- `flutter run -t lib/main.dart --flavor=host` - Host-only UI build
- Main.dart routes differently based on flavor
- Uses same Rust binary for both variants
- Simplest to maintain

**Changes Required**:
1. Create `android/build.gradle` flavor variants
2. Create `ios/Runner.xcconfig` flavor configurations
3. Modify `lib/main.dart` to accept flavor parameter
4. Conditional UI initialization based on flavor

**Pros**:
- Minimal code changes
- Single Rust binary deployment
- Configuration-based mode (already exists)
- Can coexist with full version

**Cons**:
- Still includes unused Rust client code in binary
- Slightly larger binary than option B

### Option B: Compile-Time Feature Flags (Maximum Optimization)

Build separate Rust library with `--features host-only`:

```bash
cargo build --release --features flutter,host-only
```

**Changes Required**:
1. Add `host-only` feature flag to Cargo.toml
2. Wrap `src/client.rs` and related modules in `#[cfg(not(feature = "host-only"))]`
3. Wrap client-only library exports in feature guards
4. Update `build.py` to accept feature flag parameters
5. Create separate Flutter flavor that uses host-only binary

**Pros**:
- Smallest binary size (deadcode elimination)
- Most performant
- Clean architectural separation at compile time
- Works with tree-shaking

**Cons**:
- More code changes required
- Two separate build configurations
- Requires platform-specific build script updates

### Option C: Hybrid Approach (Best Long-Term)

Combine options A & B:
1. Use compile-time feature flags to remove client code
2. Use Flutter flavors for UI/configuration
3. Maintain single source tree

**Build Commands**:
```bash
# Host-only variant
python3 build.py --flutter --flavor=host --host-only

# Full variant
python3 build.py --flutter --flavor=full
```

---

## Detailed Removal & Modification Plan

### Rust Code Changes

#### **DELETE (Host-Only)**
- `src/client.rs` - Client connection handler
- `src/client/` directory - All client modules
- `src/port_forward.rs` - TCP tunnel client
- `src/cli.rs` - CLI mode for outgoing connections

#### **MODIFY (Conditional Compilation)**
- `src/main.rs` - Remove client-only entry points
- `src/lib.rs` - Conditional module exports
- `src/flutter_ffi.rs` - Remove client-specific FFI bindings:
  - `client_send_option`
  - `client_get_option`
  - Connection list APIs
  - Remote device scan
  - File transfer initiation
- `src/flutter.rs` - Conditional FFI handler registration
- `src/ui_session_interface.rs` - Session handling (mostly server-side)
- `src/platform/windows.rs` - Remove client-specific tray menu items
- `src/platform/macos.rs` - Remove client-specific window handling
- `src/platform/linux.rs` - Remove client-specific features

#### **KEEP (Host-Only)**
- `src/server.rs` - Core server
- `src/server/` - All services
- `src/rendezvous_mediator.rs` - Device registration
- `src/auth_2fa.rs` - Authentication
- `src/common.rs` - Common utilities
- `src/keyboard.rs` - Input handling
- `src/clipboard.rs` - Clipboard
- All platform-specific code can be kept (some features unused)

### Flutter Changes

#### **DELETE (Host-Only Build Flavor)**
- `flutter/lib/desktop/pages/remote_page.dart`
- `flutter/lib/desktop/pages/remote_tab_page.dart`
- `flutter/lib/desktop/screen/desktop_remote_screen.dart`
- `flutter/lib/desktop/pages/file_manager_page.dart`
- `flutter/lib/desktop/pages/terminal_page.dart`
- `flutter/lib/desktop/pages/port_forward_page.dart`
- `flutter/lib/desktop/pages/view_camera_page.dart`
- `flutter/lib/desktop/pages/view_camera_tab_page.dart`
- `flutter/lib/mobile/pages/remote_page.dart`
- `flutter/lib/mobile/pages/file_manager_page.dart`
- `flutter/lib/mobile/pages/terminal_page.dart`
- `flutter/lib/mobile/pages/view_camera_page.dart`
- `flutter/lib/mobile/pages/scan_page.dart`

#### **MODIFY (Host-Only)**
- `flutter/lib/main.dart` - Remove multi-window client types:
  - `WindowType.RemoteDesktop`
  - `WindowType.FileTransfer`
  - `WindowType.ViewCamera`
  - `WindowType.PortForward`
  - `WindowType.Terminal`
  - Keep only `WindowType.Main` and `WindowType.ConnectionManager`

- `flutter/lib/desktop/pages/desktop_tab_page.dart` - Host tabs only (server, settings)
- `flutter/lib/desktop/pages/connection_page.dart` - Keep as is (host connections)
- `flutter/lib/desktop/pages/desktop_home_page.dart` - Remove right pane (remote controls)
- `flutter/lib/desktop/pages/install_page.dart` - May need host-only mode check

- `flutter/lib/models/state_model.dart` - Remove client-side window state
- `flutter/lib/common/widgets/` - Remove remote viewer widgets

#### **KEEP (Host-Only)**
- `flutter/lib/desktop/pages/server_page.dart` - Connection Manager
- `flutter/lib/desktop/pages/desktop_setting_page.dart` - Settings (host only)
- `flutter/lib/desktop/widgets/` - Shared widgets
- `flutter/lib/mobile/pages/server_page.dart` - Mobile host
- `flutter/lib/mobile/pages/home_page.dart` - Mobile home
- `flutter/lib/common/` - Shared utilities
- `flutter/lib/models/server_model.dart` - Host connections
- `flutter/lib/models/chat_model.dart` - Chat (server side)

### Library Changes

#### **Libraries to Remove Dependencies**
- `libs/enigo/` - Input simulation device
  - Currently only used by client
  - Host-only can remove this dependency
  - Saves build time and binary size

#### **Libraries to Keep**
- `libs/hbb_common/` - Core protocol, config, networking
- `libs/scrap/` - Video capture (host needs this)
- `libs/clipboard/` - Clipboard sharing (host needs this)
- `libs/virtual_display/` - Windows display management
- `libs/remote_printer/` - Windows printer support

---

## Configuration & Runtime Behavior

### Host-Only Mode Activation

The application can run in host-only mode via configuration file:

**Option 1: Hard Settings File** (recommended for appliances)
```
# /etc/shopremote2/hard-settings.toml (Linux)
# or registry (Windows)
conn-type=incoming
disable-ab=Y                # Disable address book (optional)
disable-cm=Y               # Disable connection manager window toggle (optional)
```

**Option 2: Environment Variable** (for containerized deployments)
```bash
export SHOPREMOTE2_CONN_TYPE=incoming
```

**Option 3: Command Line Argument** (for flexible deployments)
```bash
shopremote2 --conn-type=incoming
```

### UI Behavior in Host-Only Mode

When `conn-type=incoming` is set:
- **Main Window** shows:
  - Device ID and password
  - Connected clients list
  - Status indicator
  - Simple settings (quality, clipboard enable, etc.)
- **Hidden Elements**:
  - Address book / contact list (no connections to maintain)
  - Remote control pages
  - File transfer UI
  - Port forwarding
  - Terminal client
- **Tray Menu** restricted to:
  - Show/Hide main window
  - Settings
  - Quit
  - No "Add peer" or "Connect to" options

---

## Key Risks & Considerations

### 1. **Code Coupling Between Client & Server**
**Risk**: Shared models, utilities, and UI components may have dependencies on client code

**Mitigation**:
- Audit all shared modules before deletion
- Create feature-gated stubs for heavily-coupled modules
- Use `#[cfg(feature = "host-only")]` throughout

**Affected Files to Review**:
- `src/ui_interface.rs`
- `src/ui_session_interface.rs`
- `flutter/lib/models/`
- `flutter/lib/common.dart`

### 2. **Platform-Specific Code Cleanup**
**Risk**: Windows/macOS/Linux platform code includes both client and server features

**Mitigation**:
- Keep all platform code to avoid regressions
- Use feature flags internally to skip client-only initialization
- Platform code is small price for reliability

**Files**:
- `src/platform/windows.rs` - Large file with mixed concerns
- `src/platform/macos.rs` - Desktop/dock features
- `src/platform/linux.rs` - D-Bus, X11/Wayland

### 3. **Unused Dependencies in Cargo.toml**
**Risk**: Binary size increase from unnecessary crates

**Mitigation**:
- Wrap dependency imports in `[target.'cfg(feature = "!host-only")']`
- Test release binary size
- Profile with `cargo bloat`

**Candidates for Conditional Dependency**:
- `enigo` - Input simulation
- `arboard` - Clipboard (keep - host needs receive-only)
- `portable-pty` - Terminal (only client uses)

### 4. **Build Complexity**
**Risk**: Maintaining separate build configurations

**Mitigation**:
- Document build process clearly
- Automate with `build.py` enhancements
- Use CI/CD for both variants
- Create test matrix for both

### 5. **Feature Interaction**
**Risk**: `hwcodec`, `vram`, `screencapturekit` features assume full app

**Mitigation**:
- These features are orthogonal to host/client split
- Can still be applied to host-only builds
- Keep existing feature flag system

### 6. **Plugin System Compatibility**
**Risk**: Plugins may expect client API availability

**Mitigation**:
- Host-only doesn't support plugins initially
- Document plugin restrictions
- Can add server-only plugin API later

---

## Estimated Scope & Effort

### Rust Changes: **Medium-High Effort**
- **Remove client modules**: 2-3 hours
- **Add feature flags**: 1-2 hours
- **Update FFI bindings**: 2-3 hours
- **Platform code cleanup**: 2-3 hours
- **Testing & debugging**: 3-4 hours
- **Subtotal**: ~12-15 hours

### Flutter Changes: **Low-Medium Effort**
- **Remove client pages**: 1-2 hours
- **Modify routing in main.dart**: 1-2 hours
- **Clean up models & widgets**: 1-2 hours
- **Create flavor configurations**: 1-2 hours
- **Testing & UI polish**: 2-3 hours
- **Subtotal**: ~6-11 hours

### Build System & Automation: **Low Effort**
- **Update build.py**: 1-2 hours
- **Create CI/CD matrix**: 1-2 hours
- **Documentation**: 2-3 hours
- **Subtotal**: ~4-7 hours

### **Total Estimated Effort**: 22-33 hours (~1 week of development)

### Breakdown by Priority

**Phase 1 (Essential)**: 8-12 hours
- Add host-only feature flag
- Remove `src/client.rs` and related code
- Update Flutter main window routing
- Create simple flavor configuration

**Phase 2 (Cleanup)**: 6-10 hours
- Remove Flutter client pages
- Conditional compilation of platform code
- Update build.py

**Phase 3 (Optimization & Testing)**: 8-11 hours
- Remove unused dependencies
- Binary size optimization
- Cross-platform testing (Windows, macOS, Linux)
- Documentation and CI/CD setup

---

## Success Criteria

### For MVP (Minimum Viable Product)

1. **Functionality**
   - ✓ Builds successfully with `--features host-only`
   - ✓ Accepts incoming connections
   - ✓ Serves video, audio, input, clipboard streams
   - ✓ No outgoing connection UI available
   - ✓ Runs in `conn-type=incoming` mode

2. **Binary Size**
   - ✓ Windows .exe: <60 MB (vs ~90 MB full version)
   - ✓ Linux binary: <40 MB (vs ~60 MB full version)
   - ✓ macOS .app: <80 MB (vs ~120 MB full version)

3. **Deployment**
   - ✓ Can be packaged as MSI (Windows)
   - ✓ Can be packaged as DEB/RPM (Linux)
   - ✓ Can be packaged as DMG (macOS)
   - ✓ Works in Dockerfile/containerized environments

4. **Quality Assurance**
   - ✓ No compile warnings
   - ✓ Basic smoke tests pass (startup, connection)
   - ✓ Platform-specific tests pass

### For Production

1. **Hardening**
   - ✓ Error handling for missing client features
   - ✓ Graceful degradation if feature flags conflict
   - ✓ Comprehensive logging

2. **Documentation**
   - ✓ Build instructions for host-only variant
   - ✓ Deployment guide for enterprise use
   - ✓ Security hardening guidelines
   - ✓ Configuration reference

3. **Testing**
   - ✓ Regression test suite for host functionality
   - ✓ Cross-platform compatibility matrix
   - ✓ Performance benchmarks
   - ✓ Security review

---

## Implementation Roadmap

### Week 1: Foundation
- [ ] Day 1-2: Add feature flag to Cargo.toml, create conditional module stubs
- [ ] Day 2-3: Remove `src/client.rs` and `src/client/` safely
- [ ] Day 3-4: Update Flutter main.dart routing
- [ ] Day 4-5: Initial build & test

### Week 2: Cleanup & Flutter
- [ ] Day 1-2: Remove Flutter client pages (list view)
- [ ] Day 2-3: Create Flutter flavor configuration
- [ ] Day 3-4: Platform-specific code review & cleanup
- [ ] Day 4-5: Build & testing

### Week 3: Polish & Documentation
- [ ] Day 1: Binary size optimization
- [ ] Day 2: CI/CD pipeline setup
- [ ] Day 3-4: Documentation & deployment guides
- [ ] Day 5: Final testing & release preparation

---

## Deployment Strategy

### Target Use Cases

1. **Retail Point-of-Sale (POS) Systems**
   - Host-only on cash registers
   - Configure with `conn-type=incoming`
   - Disable settings UI with `disable-settings=Y`
   - Bake into POS software image

2. **Enterprise Remote Support**
   - Deploy to kiosk machines
   - Use script to set `conn-type=incoming` at provisioning
   - Lock down with device policy (Windows) or SELinux (Linux)

3. **Smart Display/Kiosk**
   - Container deployment with environment variables
   - Minimal footprint
   - Automatic reconnection on disconnect

### Packaging Options

**Windows**
- MSI installer (via WiX Toolset)
- NSIS installer
- Portable .exe

**Linux**
- Debian/Ubuntu (.deb)
- RedHat/CentOS (.rpm)
- FlatPak/Snap
- Docker image

**macOS**
- DMG installer
- Homebrew formula
- App Store (future)

---

## References

### Key Source Files
- **Config Management**: `libs/hbb_common/src/config.rs` (lines 2491-2506)
- **Flutter Entry**: `flutter/lib/main.dart`
- **Server Core**: `src/server.rs`
- **Client Core**: `src/client.rs`
- **FFI Bridge**: `src/flutter_ffi.rs`
- **Build Script**: `build.py`

### Related Documentation
- `CLAUDE.md` - Build commands and development setup
- `README.md` - Project overview and installation
- `docs/SECURITY.md` - Security considerations
- Cargo.toml feature flags (lines 23-43)

---

## Questions & Next Steps

### Clarification Needed (For Stakeholders)

1. **Binary Size Target**: What is the acceptable host-only binary size?
2. **Configuration Flexibility**: Should host-only mode be runtime-configurable or baked-in?
3. **Plugin Support**: Should host-only builds support plugins?
4. **Mobile Priority**: Is mobile host-only variant needed (Android/iOS)?
5. **Update Mechanism**: Should host-only versions auto-update?

### Next Phase Deliverables

Upon approval, implementation phase will produce:

1. **Source Code**
   - Feature-flagged Rust code
   - Host-only Flutter flavor
   - Updated build system

2. **Artifacts**
   - Host-only binaries for Windows, macOS, Linux
   - Binary size comparison report
   - Feature parity checklist

3. **Documentation**
   - Build instructions
   - Deployment guide
   - Configuration reference
   - Troubleshooting guide

4. **Testing**
   - Test plan and results
   - Performance benchmarks
   - Cross-platform verification matrix

---

**Document Version**: 1.0
**Last Updated**: April 2, 2026
**Status**: Ready for Review
