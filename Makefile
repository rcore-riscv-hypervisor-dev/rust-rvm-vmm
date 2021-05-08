STRIP=$(ARCH)-linux-musl-strip
.PHONY: strip dtb
strip:
	$(STRIP) target/$(ARCH)-rcore/$(MODE)/rust-rvm-vmm -o target/$(ARCH)-rcore/$(MODE)/rust-rvm-vmm-strip
dtb: rust-rvm-vmm-devices/src/board/rcore_on_rcore.dtb
rust-rvm-vmm-devices/src/board/rcore_on_rcore.dtb: rust-rvm-vmm-devices/src/board/rcore_on_rcore.dts
	dtc rust-rvm-vmm-devices/src/board/rcore_on_rcore.dts -O dtb -o rust-rvm-vmm-devices/src/board/rcore_on_rcore.dtb