rust-rvm-vmm
==============

VMM using [RVM](https://github.com/rcore-os/RVM) running on [rCore](https://github.com/rcore-os/rCore) ([rcore-vmm](https://github.com/rcore-os/rcore-vmm) equivalent), but written in rust.

Supported architectures: riscv64

rust-rvm-vmm-devices
--------------
Standalone crate for some useful devices. Moved into separate crate for easy testing.

```
cd rust-rvm-vmm-devices && cargo test --target=x86_64-unknown-linux-gnu
```