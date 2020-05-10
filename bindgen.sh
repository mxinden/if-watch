#!/bin/sh --
set -euo pipefail
unset path
case $0 in
   /*) path=${0%/*};;
   */*) path=$PWD/${0%/*};;
   *) path=$PWD;;
esac
mingw_include=/usr/x86_64-w64-mingw32/sys-root/mingw/include

# sudo dnf install mingw{32,64}-headers
bindgen \
   --impl-debug \
   --impl-partialeq \
   --whitelist-type '[_P]?MIB_IPFORWARD_ROW2' \
   --whitelist-var 'NO_ERROR|AF_UNSPEC' \
   --whitelist-function NotifyRouteChange2 \
   --whitelist-function CancelMibChangeNotify2 \
   --whitelist-function SleepEx \
   --whitelist-function GetCurrentThreadId \
   --no-rustfmt-bindings \
   --with-derive-eq \
   --with-derive-ord \
   --with-derive-hash \
   "$mingw_include/netioapi.h" \
   -- \
   --target=i686-w64-mingw32 \
   -include "$mingw_include/error.h" \
   -include "$mingw_include/winsock2.h" \
   -include "$mingw_include/winternl.h" |
   rustup run nightly rustfmt > src/windows/bindings.rs
