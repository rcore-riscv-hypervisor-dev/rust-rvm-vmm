STRIP=$(ARCH)-linux-musl-strip
.PHONY: strip
strip:
	$(STRIP) target/$(ARCH)-rcore/$(MODE)/rust-rvm-vmm -o target/$(ARCH)-rcore/$(MODE)/rust-rvm-vmm-strip
