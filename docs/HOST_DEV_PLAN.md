# ShopRemote2 Host-Only Build Development Plan

**Document Version**: 1.0
**Date**: 2026-04-02
**Status**: Design Phase
**Target**: Host-only (incoming-only) build for desktop platforms (Windows, macOS, Linux)

---

## Executive Summary

ShopRemote2 is a rebranded RustDesk 1.4.6 with both controller (client) and host (server) functionality. The goal is to create a **Host-only build** that acts like TeamViewer Host - a standalone application that only accepts remote connections and does not allow initiating remote connections to other machines.

**Key Finding**: The codebase already has partial support for incoming/outgoing-only modes through the `conn-type` hardcoded setting in `hbb_common/src/config.rs`. However, this support is incomplete and scattered throughout the codebase. This plan consolidates that work and provides a complete implementation path.

---

## 1. Architecture Analysis

### 1.1 Rust Side Architecture

#### Main Module Organization (src/lib.rs)
- **Host-only modules** (needed):
  - `server.rs` & `src/server/*` - Server/host functionality
  - `ipc.rs` - Inter-process communication for UI-backend
  - `common.rs` - Shared utilities, config, constants
  - `platform/*` - Platform-specific host implementation
  - `privacy_mode.rs` - Privacy mode for host
  - `keyboard.rs` - Input handling
  - `clipboard.rs` - Clipboard sync
  - `lang.rs` - Localization
  - `tray.rs` - System tray

- **Controller/Client modules** (to exclude or stub):
  - `client.rs` & `src/client/*` - Client/controller functionality
  - `ui_session_interface.rs` - Remote session UI interface (controller-specific)
  - `port_forward.rs` - Port forwarding (client feature)
  - `whiteboard.rs` - Whiteboard collaboration (controller feature)

- **Conditional modules**:
  - `ui.rs` - Main UI logic (needs filtering)
  - `core_main.rs` - Main initialization (desktop only, needs adjustment)

#### Server Module Structure (src/server/)
```
server/
├── connection.rs - Connection handling (HOST-ESSENTIAL)
├── service.rs - Service abstraction (HOST-ESSENTIAL)
├── video_service.rs - Screen capture & encoding (HOST-ESSENTIAL)
├── audio_service.rs - Audio capture (HOST-ESSENTIAL)
├── display_service.rs - Display management (HOST-ESSENTIAL)
├── input_service.rs - Input event handling (HOST-ESSENTIAL)
├── clipboard_service.rs - Clipboard sync (HOST-ESSENTIAL)
├── terminal_service.rs - Terminal/RDP support (HOST, optional)
├── portable_service.rs - Portable deployment (HOST-ESSENTIAL on Windows)
├── printer_service.rs - Printer support (HOST, optional)
└── video_qos.rs - Video quality optimization (HOST-ESSENTIAL)
```

All server modules are needed for host-only functionality.

#### Client Module Structure (src/client/)
```
client/
├── file_trait.rs - File manager protocol
├── helper.rs - Helper utilities
├── io_loop.rs - I/O event loop
└── screenshot.rs - Screenshot capture for sending
```

All client modules can be excluded in host-only build.

#### Feature Flags in Cargo.toml
Current flags:
- `flutter` - Flutter UI support (keep)
- `cli` - CLI mode (can remove for host)
- `hwcodec` - Hardware codec acceleration (keep)
- `vram` - VRAM usage (keep)
- `plugin_framework` - Plugin support (can remove)
- `mediacodec` - Android media codec (not relevant for desktop host)
- `screencapturekit` - macOS screen capture (keep)
- `linux-pkg-config` - Linux deps (keep)
- `unix-file-copy-paste` - Unix clipboard files (keep)

**New flag needed**: `host-only` (default: false)

#### Rust Conditional Compilation Points

The codebase already uses these patterns:
```rust
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use self::server::*;  // Server already conditionally included

mod client;  // Client unconditionally included - NEEDS FIX

#[cfg(not(any(target_os = "ios")))]
mod rendezvous_mediator;  // Mediator for P2P, needs for server
```

**Current Issues**:
1. `client.rs` is always compiled - needs to be made conditional
2. Client features like `port_forward.rs`, `whiteboard.rs` are always compiled
3. No single feature flag controls host-only vs full build
4. `ui_session_interface.rs` and `ui_interface.rs` contain both host and controller logic

### 1.2 Flutter Side Architecture

#### Desktop Pages (flutter/lib/desktop/pages/)
- **Host-specific pages**:
  - `server_page.dart` - Host management interface (KEEP - show incoming connections)
  - `desktop_setting_page.dart` - Settings (KEEP - host-relevant settings only)
  - `terminal_tab_page.dart` - Terminal access (KEEP - for host terminal sharing)

- **Controller-specific pages** (EXCLUDE):
  - `remote_page.dart` - Remote desktop viewing
  - `remote_tab_page.dart` - Remote desktop tab
  - `file_manager_page.dart` - Remote file manager
  - `file_manager_tab_page.dart` - Remote file transfer tab
  - `port_forward_page.dart` - Port forwarding config
  - `port_forward_tab_page.dart` - Port forward tab
  - `view_camera_page.dart` - Remote camera view
  - `view_camera_tab_page.dart` - Remote camera tab

- **Shared pages** (KEEP):
  - `connection_page.dart` - Address book view
  - `desktop_tab_page.dart` - Tab management
  - `desktop_home_page.dart` - Main home page
  - `install_page.dart` - Installation

#### Models (flutter/lib/models/)
- **Host-specific models** (KEEP):
  - `server_model.dart` - Host server management
  - `chat_model.dart` - Chat for host sessions
  - `file_model.dart` - File operations on host (for file transfer from remote)
  - `native_model.dart` - Native platform integration

- **Controller-specific models** (EXCLUDE):
  - `input_model.dart` - Remote input simulation
  - `relative_mouse_model.dart` - Relative mouse for remote control
  - `peer_model.dart` - Peer management (address book)
  - `peer_tab_model.dart` - Peer tab management
  - `desktop_render_texture.dart` - Remote desktop rendering

- **Shared models** (KEEP):
  - `model.dart` - Core FFI model
  - `ab_model.dart` - Address book (needed for filtering, sync)
  - `state_model.dart` - Global state
  - `group_model.dart` - Group management
  - `user_model.dart` - User authentication
  - `platform_model.dart` - Platform info
  - `terminal_model.dart` - Terminal sessions
  - `printer_model.dart` - Printer configuration
  - `cm_file_model.dart` - Connection manager file operations
  - `chat_model.dart` - Chat functionality

#### Main Entry Points (flutter/lib/main.dart)
- Main application handles both controller and host modes
- Multi-window support for different connection types
- Detection of `--incoming-only` flag needed

**Strategy**: Create a conditional main entry point:
- Keep `main.dart` but add conditional compilation
- Create `lib/main_host.dart` for host-only specialized startup
- Use Dart conditional imports via `const` configuration

#### Tab System and Window Types
```dart
enum WindowType {
  RemoteDesktop,      // EXCLUDE - controller-only
  FileTransfer,       // EXCLUDE - controller-only
  ViewCamera,         // EXCLUDE - controller-only
  PortForward,        // EXCLUDE - controller-only
  Terminal,           // KEEP - host terminal sharing
  // New: HostMonitor   // ADD - for host statistics/monitoring
}

enum DesktopTabType {
  cm,                 // Connection Manager - KEEP (shows connected clients)
  client,             // EXCLUDE - controller tabs
  // ...
}
```

#### Settings Page Filtering
`desktop_setting_page.dart` is large (102KB) and contains:
- Host settings (enable/disable services, passwords)
- Controller settings (display options, quality settings)
- Shared settings (UI theme, language)

**Need**: Conditional UI sections based on `bind.isIncomingOnly()`

### 1.3 Build System Analysis

#### Python Build Script (build.py)
Current features:
- `--flutter` - Build Flutter version
- `--hwcodec` - Hardware codec
- `--vram` - VRAM support
- `--portable` - Windows portable
- `--unix-file-copy-paste` - Unix file ops
- `--skip-cargo` - Skip Rust compilation
- Custom packaging via `--package` arg

**Changes needed**:
- Add `--host-only` flag
- Add host-only package variant for each platform
- Adjust Flutter build targets based on mode
- Create separate binaries (e.g., `shopremote2-host.exe`)

#### GitHub Workflows (`.github/workflows/`)
- `flutter-build.yml` - Main Flutter build pipeline
- `ci.yml` - General CI testing
- `build.yml` - Release builds

**Changes needed**:
- Add host-only build variants to CI/CD
- Create separate release artifacts
- Test host-only binary on all platforms

### 1.4 Configuration & Hardcoded Settings

#### Existing incoming-only Support
Located in `libs/hbb_common/src/config.rs`:
```rust
pub fn is_incoming_only() -> bool {
    HARD_SETTINGS
        .read()
        .unwrap()
        .get("conn-type")
        .map_or(false, |x| x == ("incoming"))
}
```

This reads from `HARD_SETTINGS` which is built during compilation. The mechanism exists but needs to be wired end-to-end.

#### Platform-Specific Hardcoding
- Windows: `src/platform/windows.rs` - uses `is_outgoing_only()` to hide tray shortcuts
- All platforms: Config checks scattered throughout codebase

---

## 2. Recommended Approach: Hybrid Feature Flag + Build Variant

### Why This Approach?

After analyzing the codebase, the best approach is a **hybrid strategy**:
1. **Rust feature flag** (`host-only`) to exclude client code at compile-time
2. **Conditional Dart compilation** via build configuration
3. **Single binary** (not separate binaries) with hardcoded behavior
4. **Configuration at startup** via environment/config file

This avoids:
- ❌ Forking codebase (maintenance nightmare)
- ❌ Separate Flutter targets (build complexity)
- ✅ Keeps code unified for bug fixes
- ✅ Minimal conditional compilation overhead
- ✅ Clear feature boundaries

---

## 3. Implementation Strategy

### Phase 1: Rust-Side Changes (Week 1)

#### 3.1 Add Feature Flag

**File**: `Cargo.toml`

```toml
[features]
# ... existing features ...
host-only = []  # Exclude client/controller functionality

# Default features for full build
default = ["use_dasp"]

# For host-only CI/CD builds
default-host = []  # Empty - only required deps
```

#### 3.2 Refactor lib.rs for Conditional Compilation

**File**: `src/lib.rs`

**Current problematic code**:
```rust
mod client;  // Always compiled
mod port_forward;  // Always compiled
mod whiteboard;  // Always compiled
```

**Proposed changes**:
```rust
// Always needed
pub mod common;
pub mod platform;
pub mod server;
pub mod ipc;
pub mod lang;
pub mod keyboard;
pub mod clipboard;
pub mod tray;
pub mod privacy_mode;

// Conditional compilation based on feature flag
#[cfg(not(feature = "host-only"))]
mod client;

#[cfg(not(feature = "host-only"))]
mod port_forward;

#[cfg(not(feature = "host-only"))]
mod whiteboard;

#[cfg(not(feature = "host-only"))]
mod ui_session_interface;

#[cfg(not(feature = "host-only"))]
pub mod ui_interface;

// Keep for both modes
pub mod ui;
pub mod ipc;

// Desktop only (both modes)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod core_main;

// ... rest of conditional modules ...
```

**Complexity**: ⭐⭐⭐ (3/5) - Need to identify and update all client-only code paths

#### 3.3 Update UI Interface Files

**Files to update**:
- `src/ui_interface.rs` (main UI interface ~48KB)
- `src/ui_cm_interface.rs` (connection manager interface ~68KB)

**Changes needed**: Wrap client-specific methods with `#[cfg(not(feature = "host-only"))]`

For example, methods like:
```rust
#[cfg(not(feature = "host-only"))]
fn switch_view(&self, view: &str) { }

#[cfg(not(feature = "host-only"))]
fn open_file_transfer(&self, peer_id: &str) { }
```

**Complexity**: ⭐⭐⭐⭐ (4/5) - Large files with mixed logic, need careful analysis

#### 3.4 Update Flutter FFI Bridge

**File**: `src/flutter_ffi.rs` (~95KB)

This file is generated by `flutter_rust_bridge` but can be manually edited.

**Changes**:
- Wrap client-facing RPC calls with `#[cfg(not(feature = "host-only"))]`
- Examples:
  - `key_action()` - Remote keyboard control
  - `mouse_move()` - Remote mouse control
  - `handle_file_action()` - Remote file operations

**Complexity**: ⭐⭐⭐ (3/5) - Well-structured, just needs filtering

#### 3.5 Remove Client Code at Compilation

**File**: `src/client.rs` and `src/client/` directory

This file is ~153KB and contains:
```rust
#[cfg(not(feature = "host-only"))]
mod client;
```

All references to client types will be gated.

**Complexity**: ⭐⭐ (2/5) - Already modular, just gate the imports

---

### Phase 2: Flutter-Side Changes (Week 1-2)

#### 3.6 Create Host-Only Main Entry

**File**: `flutter/lib/main_host.dart` (NEW)

```dart
/// Host-only application entry point
/// Specialized for incoming-only connections only

import 'dart:async';
import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

import 'main.dart' as main_lib;
import 'common.dart';

/// Host-only main function
Future<void> main(List<String> args) async {
  // Set outgoing-only in the Rust backend
  // by passing configuration at startup

  desktopType = DesktopType.main;
  main_lib.kBootArgs = ['--host-only'];

  // Initialize window manager for desktop
  await windowManager.ensureInitialized();

  // Run host-only app
  await main_lib.initEnv(main_lib.kAppTypeMain);

  runApp(const HostOnlyApp());
}

class HostOnlyApp extends StatelessWidget {
  const HostOnlyApp({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ShopRemote2 Host',
      home: const HostHomePage(),
      // ... standard app config ...
    );
  }
}

class HostHomePage extends StatelessWidget {
  const HostHomePage({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    // Show host-only UI:
    // - Server status (ID, password, encryption)
    // - Connected clients list
    // - Settings (password, permissions)
    // - Terminal/file access for connected clients

    return Scaffold(
      appBar: AppBar(title: const Text('ShopRemote2 Host')),
      body: Center(
        child: Text(bind.isIncomingOnly()
          ? 'Host Mode: Ready to accept connections'
          : 'Error: Not running in host mode'),
      ),
    );
  }
}
```

**Complexity**: ⭐⭐ (2/5) - Straightforward application structure

#### 3.7 Conditionally Include Pages

**File**: `flutter/lib/desktop/pages/` - Create index

New file: `flutter/lib/desktop/pages/host_pages.dart`

```dart
// Pages for host-only mode
export 'desktop_home_page.dart';     // Show server ID/password
export 'server_page.dart';             // Show connected clients
export 'desktop_setting_page.dart';   // Settings (filtered)
export 'connection_page.dart';         // Address book (filtered)
export 'install_page.dart';            // Installation
export 'terminal_tab_page.dart';      // Terminal for clients
```

New file: `flutter/lib/desktop/pages/controller_pages.dart`

```dart
// Pages for controller mode (not used in host-only)
export 'remote_page.dart';
export 'file_manager_page.dart';
// ... etc
```

**Complexity**: ⭐⭐ (2/5) - Simple re-export organization

#### 3.8 Update desktop_home_page.dart

**File**: `flutter/lib/desktop/pages/desktop_home_page.dart`

Current code at line 62-69:
```dart
final isIncomingOnly = bind.isIncomingOnly();
return _buildBlock(
    child: Row(
  crossAxisAlignment: CrossAxisAlignment.start,
  children: [
    buildLeftPane(context),
    if (!isIncomingOnly) const VerticalDivider(width: 1),
    if (!isIncomingOnly) Expanded(child: buildRightPane(context)),
  ],
));
```

This already has the right structure! The host-only UI hides the right pane (remote connection list) and only shows the left pane (server ID/password).

**Changes**: Enhance host-specific sections:
```dart
Widget buildLeftPane(BuildContext context) {
  final isIncomingOnly = bind.isIncomingOnly();
  final isOutgoingOnly = bind.isOutgoingOnly();

  final children = <Widget>[
    if (isIncomingOnly) buildServerStatus(context),  // NEW
    if (isIncomingOnly) buildClientsList(context),   // NEW
    if (!isOutgoingOnly) buildIDBoard(context),
    if (!isOutgoingOnly) buildPasswordBoard(context),
    // ... rest
  ];

  return SingleChildScrollView(
    controller: _leftPaneScrollController,
    child: Column(children: children),
  );
}
```

**Complexity**: ⭐⭐⭐ (3/5) - Need to enhance UI, create new widgets

#### 3.9 Filter Settings Page

**File**: `flutter/lib/desktop/pages/desktop_setting_page.dart`

Large file (102KB) with mixed host/controller settings.

**Approach**: Wrap sections with conditional rendering:

```dart
Widget buildGeneralSettings(BuildContext context) {
  final isIncomingOnly = bind.isIncomingOnly();

  return Column(
    children: [
      // Host settings
      if (isIncomingOnly) ...[
        buildHostPasswordSettings(),
        buildHostAccessControl(),
        buildHostServiceSettings(),
      ],

      // Controller settings (not shown for host)
      if (!isIncomingOnly) ...[
        buildRemoteDisplaySettings(),
        buildKeyboardSettings(),
      ],

      // Shared settings
      buildThemeSettings(),
      buildLanguageSettings(),
    ],
  );
}
```

**Complexity**: ⭐⭐⭐ (3/5) - Need to identify and group settings correctly

#### 3.10 Create Host-Specific Widgets

**New files**:
- `flutter/lib/desktop/widgets/server_status_widget.dart`
- `flutter/lib/desktop/widgets/connected_clients_list.dart`

These widgets show:
- Host ID with copy button
- Current password (temporary + permanent)
- Active client connections (count, IP, connection time)
- Quick action buttons (disconnect client, show logs)

**Complexity**: ⭐⭐⭐ (3/5) - Need to design new UI components

#### 3.11 Update App Initialization

**File**: `flutter/lib/common.dart`

Update platform initialization:
```dart
Future<void> initDesktopPlatform(String appType) async {
  if (bind.isIncomingOnly()) {
    // Host-only initialization
    await platformFFI.init(kAppTypeMain);
    gFFI.serverModel.startService();  // Start server immediately
    // Don't load address book, etc.
  } else {
    // Standard initialization
    await platformFFI.init(appType);
  }
}
```

**Complexity**: ⭐⭐ (2/5) - Clear initialization paths

---

### Phase 3: Build System Changes (Week 2)

#### 3.12 Update Cargo.toml Features

**File**: `Cargo.toml`

```toml
[features]
# ... existing ...
host-only = []  # No additional dependencies, just flag

[features.host-only-default]
default = []  # No default features for host builds
```

**Complexity**: ⭐ (1/5) - Simple TOML edit

#### 3.13 Update build.py

**File**: `build.py`

Add arguments:
```python
parser.add_argument(
    '--host-only',
    action='store_true',
    help='Build host-only version (no remote control capability)'
)

parser.add_argument(
    '--host-only-name',
    type=str,
    default='shopremote2-host',
    help='Binary name for host-only build'
)
```

Add to build logic:
```python
def build_rust(features, host_only=False):
    # Build arguments
    feature_args = '+'.join(features)
    if host_only:
        feature_args = 'host-only'

    cmd = f"cargo build --release --features '{feature_args}'"
    system2(cmd)

def build_flutter(host_only=False):
    # Use different main entry point
    main_file = 'lib/main_host.dart' if host_only else 'lib/main.dart'

    # Build Flutter with environment variable
    env = os.environ.copy()
    env['FLUTTER_ENTRY_POINT'] = main_file

    system2(f"flutter build {platform} --release", env=env)

# In main build logic
if args.host_only:
    build_rust([], host_only=True)
    build_flutter(host_only=True)
    # Package as shopremote2-host
    package_binary(args.host_only_name)
```

**Complexity**: ⭐⭐⭐ (3/5) - Need to integrate build pipeline

#### 3.14 Update GitHub Workflows

**File**: `.github/workflows/flutter-build.yml`

Add job matrix for host-only build:
```yaml
jobs:
  build:
    strategy:
      matrix:
        build_type: [full, host-only]
    steps:
      - name: Build ShopRemote2
        run: |
          if [ "${{ matrix.build_type }}" = "host-only" ]; then
            python3 build.py --flutter --host-only
          else
            python3 build.py --flutter
          fi

      - name: Upload Host-Only Artifacts
        if: matrix.build_type == 'host-only'
        uses: actions/upload-artifact@v3
        with:
          name: shopremote2-host-${{ runner.os }}
          path: build/*/release/*host*
```

**Complexity**: ⭐⭐ (2/5) - Standard CI/CD patterns

---

### Phase 4: Testing & Polish (Week 3)

#### 3.15 Create Test Suite

**Files to create**:
- `tests/host_only_tests.rs` - Verify host-only features compile
- `tests/integration_host.rs` - Integration tests for host mode
- `tests/flutter_host_integration.dart` - Flutter widget tests

**Test coverage**:
- Host can start server and listen for connections
- UI hides controller features
- Settings page shows only host options
- Client code is not compiled in (`#[cfg(test)]` verification)

**Complexity**: ⭐⭐⭐⭐ (4/5) - Comprehensive testing setup

#### 3.16 Documentation

**Files to create**:
- `docs/HOST_BUILD.md` - How to build host-only version
- `docs/HOST_DEPLOYMENT.md` - Deployment guide for stores
- `docs/HOST_ARCHITECTURE.md` - Technical architecture

**Complexity**: ⭐⭐ (2/5) - Documentation writing

---

## 4. Detailed File-by-File Changes

### Rust Files

| File | Action | Complexity | Impact |
|------|--------|-----------|--------|
| `Cargo.toml` | Add `host-only` feature | ⭐ | Build system |
| `src/lib.rs` | Gate client modules | ⭐⭐ | Core compilation |
| `src/client.rs` | Wrap with `#[cfg(not(feature = "host-only"))]` | ⭐ | ~153KB excluded |
| `src/client/*` | All wrapped | ⭐⭐ | ~300KB total excluded |
| `src/port_forward.rs` | Wrap with feature gate | ⭐ | ~8KB excluded |
| `src/whiteboard.rs` | Wrap with feature gate | ⭐ | ~18KB excluded |
| `src/ui_interface.rs` | Selective method gating | ⭐⭐⭐ | ~48KB, many edits |
| `src/ui_cm_interface.rs` | Selective method gating | ⭐⭐⭐ | ~68KB, many edits |
| `src/flutter_ffi.rs` | Gate client RPC methods | ⭐⭐⭐ | ~95KB, critical path |
| `src/ui.rs` | Already has `is_incoming_only()` impl | ⭐ | Use existing |
| `src/core_main.rs` | Add host-only startup path | ⭐⭐ | Startup logic |
| `build.rs` | Add host-only feature check | ⭐ | Build script |

### Flutter Files

| File | Action | Complexity | Impact |
|------|--------|-----------|--------|
| `lib/main.dart` | Add conditional initialization | ⭐⭐ | Entry point |
| `lib/main_host.dart` | Create host-specific entry | ⭐⭐ | NEW FILE |
| `lib/desktop/pages/desktop_home_page.dart` | Enhance host UI sections | ⭐⭐⭐ | ~37KB, key UI |
| `lib/desktop/pages/desktop_setting_page.dart` | Filter host/controller settings | ⭐⭐⭐ | ~102KB, large |
| `lib/desktop/pages/server_page.dart` | Already host-focused | ⭐ | ~48KB, check |
| `lib/desktop/widgets/server_status_widget.dart` | Create new widget | ⭐⭐ | NEW FILE |
| `lib/desktop/widgets/connected_clients_list.dart` | Create new widget | ⭐⭐ | NEW FILE |
| `lib/desktop/widgets/host_quick_actions.dart` | Create new widget | ⭐⭐ | NEW FILE |
| `lib/models/server_model.dart` | Already host-focused | ⭐ | ~28KB, check |
| `lib/models/model.dart` | Add host-only initialization | ⭐⭐ | ~138KB, large |
| `lib/common.dart` | Conditional initialization | ⭐⭐ | ~8KB edit |
| `flutter/pubspec.yaml` | No changes needed | - | - |

### Build System Files

| File | Action | Complexity | Impact |
|------|--------|-----------|--------|
| `build.py` | Add `--host-only` flag | ⭐⭐⭐ | ~650 lines |
| `.github/workflows/flutter-build.yml` | Add host-only matrix | ⭐⭐ | ~2000 lines |
| `Dockerfile` | Create Docker build variant | ⭐⭐ | 17 lines |

---

## 5. Implementation Schedule

### Week 1: Foundation (Rust)
- Day 1: Add feature flag, update Cargo.toml
- Day 2-3: Gate client modules in src/lib.rs, src/client.rs
- Day 4-5: Update UI interface files, test compilation
- Day 5: Verify no runtime regressions

**Deliverable**: `cargo build --features host-only` works

### Week 2: Flutter Frontend
- Day 1-2: Create host-specific entry point, widgets
- Day 3-4: Update desktop_home_page.dart, settings page
- Day 5: Test UI with mock host mode
- Day 5: Styling and polish

**Deliverable**: Host-only Flutter UI renders correctly

### Week 3: Build System & Testing
- Day 1-2: Update build.py with --host-only flag
- Day 3: GitHub Actions workflow for host-only builds
- Day 4: Integration testing (Rust + Flutter)
- Day 5: Documentation

**Deliverable**: `build.py --flutter --host-only` creates working binary

### Week 4: Polish & Deployment
- Day 1-2: Bug fixes from testing
- Day 3: Performance optimization
- Day 4: Create deployment packages (MSI, DEB, DMG)
- Day 5: Release documentation

**Deliverable**: Production-ready host-only binaries for Windows, Linux, macOS

---

## 6. Technical Risks & Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Client code has hidden dependencies on host code | Medium | High | Comprehensive testing, feature gate verification |
| Flutter conditional compilation breaks UI | Medium | High | Manual UI testing, regression tests |
| Build system complexity increases | Medium | Medium | Document build process, clear CI/CD |
| Binary size regression | Low | Medium | Monitor binary sizes, profile builds |
| iOS/Android code breaks | Low | Low | Ensure host-only feature doesn't affect mobile |

---

## 7. Success Criteria

### Build Level
- ✓ `cargo build --features host-only --target x86_64-pc-windows-msvc` succeeds
- ✓ `cargo build --features host-only --target x86_64-unknown-linux-gnu` succeeds
- ✓ `cargo build --features host-only --target x86_64-apple-darwin` succeeds
- ✓ Client code not linked when `host-only` feature enabled
- ✓ Binary size reduction of 10-20MB on Windows (client modules excluded)

### Runtime Level
- ✓ Host-only binary starts and shows server ID
- ✓ Host can accept incoming connections
- ✓ Host cannot initiate outgoing connections
- ✓ Remote controller can connect to host
- ✓ All remote functions (screen share, file transfer, etc.) work from controller

### UI/UX Level
- ✓ Host-only UI shows server status (ID, password, encryption)
- ✓ Host-only UI shows connected clients list
- ✓ Host-only settings page hides controller options
- ✓ Window title shows "ShopRemote2 Host"
- ✓ Tray menu only shows host-relevant options

### Testing Level
- ✓ Unit tests pass for host-only builds
- ✓ Integration tests verify host functionality
- ✓ No regression in full build mode
- ✓ CI/CD produces both full and host-only artifacts

---

## 8. Future Enhancements (Post-MVP)

1. **Host Service Mode**
   - Run host as Windows Service (already framework exists)
   - Systemd service for Linux
   - LaunchAgent for macOS

2. **Silent Installation**
   - MSI with `/quiet` flag for store deployments
   - Ansible playbooks for Linux fleet deployment

3. **Hardware Acceleration**
   - NVIDIA NVENC for video encoding
   - AMD VCE support
   - Intel QuickSync

4. **Mobile Receiver**
   - iOS/Android host (receive remote support on mobile)
   - Use different Flutter entry point

5. **Branding Customization**
   - Splash screen customization
   - Icon/logo replacement
   - Custom color schemes

6. **Monitoring Dashboard**
   - View host health/stats
   - Bandwidth monitoring
   - Connection audit logs

---

## 9. References

### Code References
- Feature flags: `Cargo.toml` lines 23-43
- Config system: `libs/hbb_common/src/config.rs`
- Server module: `src/server.rs` lines 1-80
- Client module: `src/client.rs` lines 1-100
- UI interface: `src/ui.rs` (has `is_incoming_only()`)
- Flutter entry: `flutter/lib/main.dart` lines 40-120

### Existing Patterns
- Mobile vs desktop: `#[cfg(not(any(target_os = "android", target_os = "ios")))]`
- CLI mode: `#[cfg(feature = "cli")]` in src/main.rs
- Flutter feature: `#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]`

---

## 10. Appendix: Quick Reference

### Build Commands

```bash
# Full build (normal)
cargo build --release
flutter build windows/linux/macos --release

# Host-only build
cargo build --release --features host-only
flutter build windows/linux/macos --release

# With build.py
python3 build.py --flutter                    # Full build
python3 build.py --flutter --host-only       # Host-only build
```

### Testing the Host-Only Build

```bash
# Start host
./target/release/shopremote2

# From another machine, connect to host's ID
# (Host acts as a server accepting connections)
```

### Debug Feature Flag

```bash
# Verify feature gate is applied
cargo tree --features host-only | grep -E "client|whiteboard|port_forward"
# Should return nothing

cargo tree | grep -E "client|whiteboard|port_forward"
# Should show these modules
```

---

**Document Status**: Complete Design Phase
**Ready for**: Implementation Phase (Week 1 of 4)
**Estimated Effort**: 160-200 developer hours (4 weeks, 1 developer)
**Risk Level**: Medium (complex codebase, feature interaction points)
