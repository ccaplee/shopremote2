import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:shopremote2/common.dart';
import 'package:shopremote2/consts.dart';
import 'package:shopremote2/desktop/pages/desktop_setting_page.dart';
import 'package:shopremote2/desktop/pages/desktop_tab_page.dart';
import 'package:shopremote2/models/server_model.dart';
import 'package:shopremote2/models/state_model.dart';
import 'package:get/get.dart';
import 'package:provider/provider.dart';

/// Host-only home page - simplified UI showing only server/host functionality
/// No remote control or client features
class DesktopHomePageHost extends StatefulWidget {
  const DesktopHomePageHost({Key? key}) : super(key: key);

  @override
  State<DesktopHomePageHost> createState() => _DesktopHomePageHostState();
}

class _DesktopHomePageHostState extends State<DesktopHomePageHost>
    with AutomaticKeepAliveClientMixin, WidgetsBindingObserver {
  final _leftPaneScrollController = ScrollController();
  Timer? _updateTimer;

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _updateTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return Container(
      color: Theme.of(context).colorScheme.background,
      child: buildHostOnlyPane(context),
    );
  }

  Widget buildHostOnlyPane(BuildContext context) {
    final textColor = Theme.of(context).textTheme.titleLarge?.color;
    return ChangeNotifierProvider.value(
      value: gFFI.serverModel,
      child: Container(
        width: 320.0,
        color: Theme.of(context).colorScheme.background,
        child: Stack(
          children: [
            Column(
              children: [
                SingleChildScrollView(
                  controller: _leftPaneScrollController,
                  child: Column(
                    children: [
                      buildLogoSection(context),
                      buildTitleSection(context),
                      buildIDSection(context),
                      buildPasswordSection(context),
                      buildStatusSection(context),
                      buildConnectionLogSection(context),
                    ],
                  ),
                ),
                Expanded(child: Container())
              ],
            ),
            // Settings button in bottom left
            Positioned(
              bottom: 12,
              left: 12,
              child: buildSettingsButton(context, textColor),
            )
          ],
        ),
      ),
    );
  }

  Widget buildLogoSection(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 24.0),
      child: Center(
        child: loadLogo(),
      ),
    );
  }

  Widget buildTitleSection(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 20.0, vertical: 12.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            translate("Your Desktop"),
            style: Theme.of(context).textTheme.titleLarge?.copyWith(
              fontSize: 18,
              fontWeight: FontWeight.w600,
            ),
          ),
          SizedBox(height: 8.0),
          Text(
            translate("Host-only mode - Allow incoming connections"),
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }

  Widget buildIDSection(BuildContext context) {
    final model = gFFI.serverModel;
    final textColor = Theme.of(context).textTheme.titleLarge?.color;
    return Container(
      margin: const EdgeInsets.only(left: 20, right: 16, top: 16, bottom: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: MyTheme.accent.withOpacity(0.3)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            translate("Device ID"),
            style: TextStyle(
              fontSize: 12,
              color: textColor?.withOpacity(0.6),
              fontWeight: FontWeight.w500,
            ),
          ),
          SizedBox(height: 8),
          GestureDetector(
            onDoubleTap: () {
              Clipboard.setData(ClipboardData(text: model.serverId.text));
              showToast(translate("Copied"));
            },
            child: Container(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: SelectableText(
                model.serverId.text,
                style: TextStyle(
                  fontSize: 20,
                  fontWeight: FontWeight.bold,
                  color: MyTheme.accent,
                  fontFamily: 'monospace',
                ),
              ),
            ),
          ),
          SizedBox(height: 8),
          Text(
            translate("Double-click to copy"),
            style: TextStyle(
              fontSize: 10,
              color: textColor?.withOpacity(0.5),
              fontStyle: FontStyle.italic,
            ),
          ),
        ],
      ),
    );
  }

  Widget buildPasswordSection(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: gFFI.serverModel,
      child: Consumer<ServerModel>(
        builder: (context, model, child) {
          return buildPasswordSection2(context, model);
        },
      ),
    );
  }

  Widget buildPasswordSection2(BuildContext context, ServerModel model) {
    final textColor = Theme.of(context).textTheme.titleLarge?.color;
    final RxBool refreshHover = false.obs;

    return Container(
      margin: const EdgeInsets.only(left: 20, right: 16, bottom: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: MyTheme.accent.withOpacity(0.3)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                translate("One-time Password"),
                style: TextStyle(
                  fontSize: 12,
                  color: textColor?.withOpacity(0.6),
                  fontWeight: FontWeight.w500,
                ),
              ),
              InkWell(
                onTap: () => bind.mainUpdateTemporaryPassword(),
                child: Tooltip(
                  message: translate('Refresh Password'),
                  child: Obx(
                    () => Icon(
                      Icons.refresh,
                      size: 18,
                      color: refreshHover.value
                          ? MyTheme.accent
                          : textColor?.withOpacity(0.5),
                    ),
                  ),
                ),
                onHover: (value) => refreshHover.value = value,
              ),
            ],
          ),
          SizedBox(height: 8),
          GestureDetector(
            onDoubleTap: () {
              Clipboard.setData(
                ClipboardData(text: model.serverPasswd.text),
              );
              showToast(translate("Copied"));
            },
            child: Container(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: SelectableText(
                model.serverPasswd.text,
                style: TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.bold,
                  color: MyTheme.accent,
                  fontFamily: 'monospace',
                ),
              ),
            ),
          ),
          SizedBox(height: 8),
          Text(
            translate("Double-click to copy"),
            style: TextStyle(
              fontSize: 10,
              color: textColor?.withOpacity(0.5),
              fontStyle: FontStyle.italic,
            ),
          ),
        ],
      ),
    );
  }

  Widget buildStatusSection(BuildContext context) {
    final textColor = Theme.of(context).textTheme.titleLarge?.color;
    return Container(
      margin: const EdgeInsets.only(left: 20, right: 16, bottom: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: MyTheme.accent.withOpacity(0.3)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            translate("Service Status"),
            style: TextStyle(
              fontSize: 12,
              color: textColor?.withOpacity(0.6),
              fontWeight: FontWeight.w500,
            ),
          ),
          SizedBox(height: 8),
          Consumer<ServerModel>(
            builder: (context, model, child) {
              return Obx(() {
                final stopped = Get.find<RxBool>(tag: 'stop-service');
                final isRunning = !stopped.value;
                final statusColor =
                    isRunning ? Colors.green : Colors.red;
                return Row(
                  children: [
                    Container(
                      width: 12,
                      height: 12,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: statusColor,
                      ),
                    ),
                    SizedBox(width: 8),
                    Text(
                      isRunning ? translate("Running") : translate("Stopped"),
                      style: TextStyle(
                        fontSize: 14,
                        color: statusColor,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ],
                );
              });
            },
          ),
        ],
      ),
    );
  }

  Widget buildConnectionLogSection(BuildContext context) {
    final textColor = Theme.of(context).textTheme.titleLarge?.color;
    return Container(
      margin: const EdgeInsets.only(left: 20, right: 16, bottom: 16),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        border: Border.all(color: MyTheme.accent.withOpacity(0.3)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            translate("Recent Connections"),
            style: TextStyle(
              fontSize: 12,
              color: textColor?.withOpacity(0.6),
              fontWeight: FontWeight.w500,
            ),
          ),
          SizedBox(height: 8),
          Container(
            constraints: BoxConstraints(maxHeight: 150),
            child: SingleChildScrollView(
              child: Consumer<ServerModel>(
                builder: (context, model, child) {
                  // Get recent connection info from model
                  final connections = model.clients;
                  if (connections.isEmpty) {
                    return Text(
                      translate("No recent connections"),
                      style: TextStyle(
                        fontSize: 12,
                        color: textColor?.withOpacity(0.5),
                        fontStyle: FontStyle.italic,
                      ),
                    );
                  }
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: connections
                        .take(5)
                        .map((conn) => Padding(
                          padding: const EdgeInsets.symmetric(vertical: 4.0),
                          child: Text(
                            "${conn.name} (${conn.peerId})",
                            style: TextStyle(
                              fontSize: 11,
                              color: textColor?.withOpacity(0.7),
                            ),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ))
                        .toList(),
                  );
                },
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget buildSettingsButton(BuildContext context, Color? textColor) {
    final RxBool settingsHover = false.obs;
    return Tooltip(
      message: translate('Settings'),
      child: InkWell(
        onTap: () => DesktopTabPage.onAddSetting(),
        child: Obx(
          () => Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: settingsHover.value
                  ? Theme.of(context).scaffoldBackgroundColor
                  : Theme.of(context).colorScheme.background,
              border: Border.all(
                color: settingsHover.value
                    ? MyTheme.accent.withOpacity(0.5)
                    : Colors.transparent,
              ),
            ),
            child: Icon(
              Icons.settings,
              size: 20,
              color: settingsHover.value
                  ? MyTheme.accent
                  : textColor?.withOpacity(0.5),
            ),
          ),
        ),
        onHover: (value) => settingsHover.value = value,
      ),
    );
  }
}
