Name:           moltis
Version:        0.1.0
Release:        1%{?dist}
Summary:        Rust version of moltbot
License:        MIT
URL:            https://docs.molt.bot/

%description
Moltis is a Rust implementation of moltbot, a multi-feature bot system.

%install
mkdir -p %{buildroot}%{_bindir}
install -m 755 %{_sourcedir}/moltis %{buildroot}%{_bindir}/moltis

%files
%{_bindir}/moltis
