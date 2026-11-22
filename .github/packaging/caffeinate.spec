%global debug_package %{nil}
%global _build_id_links none
%global __brp_strip %{nil}
%global __brp_strip_comment_note %{nil}

Name: caffeinate
Version: %{package_version}
Release: 1
Summary: Keep your computer awake from the command line
License: (MIT OR Apache-2.0) AND AFL-2.1
URL: https://github.com/Rajaniraiyn/caffinate
AutoReq: no

%description
Keep the computer awake while a command runs or until a timeout.

%install
install -Dm755 "%{binary}" "%{buildroot}%{_bindir}/caffeinate"
install -Dm644 -t "%{buildroot}%{_licensedir}/%{name}" \
  "%{project_dir}/LICENSE-MIT" "%{project_dir}/LICENSE-APACHE" "%{dbus_license}"

%files
%{_bindir}/caffeinate
%license %{_licensedir}/%{name}/LICENSE-*
