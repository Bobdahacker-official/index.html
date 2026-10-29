# Third-party components

- Limine v9.x bootloader, BSD-2-Clause: https://github.com/limine-bootloader/limine/tree/v9.x-binary . The vendored UEFI executable is in `vendor/limine/BOOTX64.EFI`; its license is in `vendor/limine/LICENSE`.
- DejaVu Sans, Bitstream Vera permissive font license with public-domain DejaVu changes: https://dejavu-fonts.github.io/ . The font and license are in `vendor/fonts/`; `assets/font.rle` and `assets/widths.bin` are rasterized derivatives.
- Rust 1.90.0 compiler and core/alloc runtime, MIT OR Apache-2.0: https://www.rust-lang.org/policies/licenses . The compiler is a build prerequisite; compiler-provided core/alloc routines are linked into the kernel.
- Pillow, HPND: https://python-pillow.github.io/ . Used only to regenerate the font atlas with `scripts/font.py`; not included in the operating-system image and not required for ordinary builds.

The ISO, FAT image, GPT, graphics, application, input, allocator, and storage code in this tree is original. No Linux kernel or Linux userland is included in the boot images. QEMU and OVMF are external execution tools and are not part of these images.
