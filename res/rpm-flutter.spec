Name:       shopremote2
Version:    1.4.6
Release:    0
Summary:    RPM package
License:    GPL-3.0
URL:        https://github.com/ccaplee/shopremote2
Vendor:     shopremote2 <ccccap@naver.com>
Requires:   gtk3 libxcb libXfixes alsa-lib libva pam gstreamer1-plugins-base
Recommends: libayatana-appindicator-gtk3 libxdo
Provides:   libdesktop_drop_plugin.so()(64bit), libdesktop_multi_window_plugin.so()(64bit), libfile_selector_linux_plugin.so()(64bit), libflutter_custom_cursor_plugin.so()(64bit), libflutter_linux_gtk.so()(64bit), libscreen_retriever_plugin.so()(64bit), libtray_manager_plugin.so()(64bit), liburl_launcher_linux_plugin.so()(64bit), libwindow_manager_plugin.so()(64bit), libwindow_size_plugin.so()(64bit), libtexture_rgba_renderer_plugin.so()(64bit)

# https://docs.fedoraproject.org/en-US/packaging-guidelines/Scriptlets/

%description
The best open-source remote desktop client software, written in Rust.

%prep
# we have no source, so nothing here

%build
# we have no source, so nothing here

# %global __python %{__python3}

%install

mkdir -p "%{buildroot}/usr/share/shopremote2" && cp -r ${HBB}/flutter/build/linux/x64/release/bundle/* -t "%{buildroot}/usr/share/shopremote2"
mkdir -p "%{buildroot}/usr/bin"
install -Dm 644 $HBB/res/shopremote2.service -t "%{buildroot}/usr/share/shopremote2/files"
install -Dm 644 $HBB/res/shopremote2.desktop -t "%{buildroot}/usr/share/shopremote2/files"
install -Dm 644 $HBB/res/shopremote2-link.desktop -t "%{buildroot}/usr/share/shopremote2/files"
install -Dm 644 $HBB/res/128x128@2x.png "%{buildroot}/usr/share/icons/hicolor/256x256/apps/shopremote2.png"
install -Dm 644 $HBB/res/scalable.svg "%{buildroot}/usr/share/icons/hicolor/scalable/apps/shopremote2.svg"

%files
/usr/share/shopremote2/*
/usr/share/shopremote2/files/shopremote2.service
/usr/share/icons/hicolor/256x256/apps/shopremote2.png
/usr/share/icons/hicolor/scalable/apps/shopremote2.svg
/usr/share/shopremote2/files/shopremote2.desktop
/usr/share/shopremote2/files/shopremote2-link.desktop

%changelog
# let's skip this for now

%pre
# can do something for centos7
case "$1" in
  1)
    # for install
  ;;
  2)
    # for upgrade
    systemctl stop shopremote2 || true
  ;;
esac

%post
cp /usr/share/shopremote2/files/shopremote2.service /etc/systemd/system/shopremote2.service
cp /usr/share/shopremote2/files/shopremote2.desktop /usr/share/applications/
cp /usr/share/shopremote2/files/shopremote2-link.desktop /usr/share/applications/
ln -sf /usr/share/shopremote2/shopremote2 /usr/bin/shopremote2
systemctl daemon-reload
systemctl enable shopremote2
systemctl start shopremote2
update-desktop-database

%preun
case "$1" in
  0)
    # for uninstall
    systemctl stop shopremote2 || true
    systemctl disable shopremote2 || true
    rm /etc/systemd/system/shopremote2.service || true
  ;;
  1)
    # for upgrade
  ;;
esac

%postun
case "$1" in
  0)
    # for uninstall
    rm /usr/bin/shopremote2 || true
    rmdir /usr/lib/shopremote2 || true
    rmdir /usr/local/shopremote2 || true
    rmdir /usr/share/shopremote2 || true
    rm /usr/share/applications/shopremote2.desktop || true
    rm /usr/share/applications/shopremote2-link.desktop || true
    update-desktop-database
  ;;
  1)
    # for upgrade
    rmdir /usr/lib/shopremote2 || true
    rmdir /usr/local/shopremote2 || true
  ;;
esac
