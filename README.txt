# MicKey

A lightweight, portable microphone mute hotkey for Windows, written in Rust.

**[Download MicKey.exe](https://github.com/Samsxx001/MicKey/releases/latest/download/MicKey.exe)** · **[Latest release / ZIP](https://github.com/Samsxx001/MicKey/releases/latest)** · [Report an issue](https://github.com/Samsxx001/MicKey/issues)

![Microphone status icons](icon-preview.png)

โปรแกรมปิด/เปิดไมค์ด้วยปุ่มลัดสำหรับ Windows 10/11 แบบ 64 บิต ใช้หน้าต่าง Win32 โดยตรง ไม่มี Electron/WebView และไม่มีการวนอ่านปุ่มหรือสถานะไมค์ขณะว่าง

## วิธีใช้

1. ดาวน์โหลด `MicKey.exe` แล้วเปิด ไม่ต้องติดตั้ง Rust
2. ค่าเริ่มต้นคือ **Ctrl** กดแล้วปล่อยหนึ่งครั้งเพื่อสลับปิด/เปิดไมค์เริ่มต้นของ Windows
3. เปลี่ยนปุ่มด้วย **เลือกปุ่มลัด…** แล้วกดปุ่มหรือปุ่มผสม เช่น F8 หรือ Ctrl+Shift+M จากนั้นกด **บันทึกและซ่อน**
4. กด **X / Alt+F4** เพื่อซ่อนไป System Tray ปุ่มลัดยังทำงาน
5. คลิกไอคอนข้างนาฬิกาเพื่อเปิดหน้าตั้งค่า คลิกขวาเพื่อสลับไมค์ ตรวจสอบอัปเดต หรือออกจากโปรแกรม
6. ปิดจริงด้วย **ออกจากโปรแกรม** ในเมนูถาดระบบหรือหน้าตั้งค่า

ถ้าไม่เห็นไอคอน ให้กดลูกศรแสดงไอคอนที่ซ่อน หาก Windows สร้างไอคอนถาดระบบไม่ได้ แอปจะย่อบนแถบงานแทนและยังทำงานต่อ

## อัปเดตโปรแกรม

เลือก **ตรวจสอบอัปเดต…** จากหน้าตั้งค่าหรือเมนูคลิกขวาที่ไอคอน โปรแกรมตรวจ stable release จาก GitHub เมื่อสั่งเท่านั้น ไม่มีการเช็กเครือข่ายเบื้องหลังเป็นระยะ

ถ้ามีรุ่นใหม่ โปรแกรมจะถามก่อนดาวน์โหลด ตรวจขนาดและ SHA-256 ที่ GitHub รายงาน แล้วปิดและเปิดโปรแกรมรุ่นใหม่ให้ ค่าปุ่มลัดเดิมยังอยู่ที่ `%APPDATA%\MicKey\shortcut.txt`

เก็บไฟล์รุ่นก่อนเป็น `MicKey.exe.previous` หากเปลี่ยนไฟล์หรือเริ่มรุ่นใหม่ไม่สำเร็จ ตัวติดตั้งจะพยายามคืนและเปิดรุ่นเดิม ต้องเก็บโปรแกรมในโฟลเดอร์ที่บัญชี Windows ของคุณมีสิทธิ์เขียน ระบบอัปเดตใช้ Windows PowerShell ที่มากับ Windows และไม่เปลี่ยนการตั้งค่า execution policy ของระบบ

ผู้ใช้รุ่น 1.2 หรือต่ำกว่าให้ดาวน์โหลดรุ่น 1.3 ขึ้นไปด้วยตนเองครั้งแรก ปิดรุ่นเดิมก่อนเปิดรุ่นใหม่ แล้วใช้เมนูอัปเดตสำหรับรุ่นถัดไปได้

## สถานะและปุ่มลัด

- ไมค์สีเขียว = เปิดเสียง; สีแดงมีขีดทับ = ปิดเสียง; สีเทาพร้อมเครื่องหมายเตือน = ไม่พบไมค์/อ่านสถานะไม่ได้
- ใช้ Core Audio notifications เพื่อติดตามสถานะและการเปลี่ยนไมค์เริ่มต้น ไอคอนแสดง endpoint mute ของ Windows ไม่ใช่ปุ่ม mute ภายใน Discord/Zoom
- ปุ่มลัดหยุดทำงานขณะหน้าตั้งค่าเปิดอยู่ ซ่อนหน้าต่างเพื่อใช้งานต่อ
- กดค้างไม่สลับรัว ค่าเริ่มต้น Ctrl ไม่สลับไมค์เมื่อใช้ Ctrl+C, Ctrl+V, Ctrl+คลิก หรือ Ctrl+ล้อเมาส์
- รองรับปุ่มคีย์บอร์ดทั่วไปและปุ่มผสม รวมถึง Ctrl/Shift/Alt ซ้ายและขวา ไม่รองรับการตั้งปุ่มเมาส์ ปุ่ม Fn/ชุดปุ่มที่ Windows สงวนไว้อาจไม่ส่งเหตุการณ์ให้แอป
- ปุ่มส่งต่อให้แอปที่กำลังใช้ตามปกติ ยกเว้นช่วงเลือกปุ่มลัด ถ้าตั้งเป็นตัวอักษร การพิมพ์ตัวนั้นจะสลับไมค์ด้วย

ควบคุมเฉพาะไมค์เริ่มต้นของ Windows แอปที่เลือกไมค์คนละตัวจะไม่ได้รับผล อุปกรณ์ที่ไม่มี hardware mute อาจไม่ปิดเสียงให้แอปที่ใช้ exclusive mode ตามข้อจำกัดของ [Windows EndpointVolume API](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudioendpointvolume)

ไม่มีการบันทึกเสียงหรือประวัติการพิมพ์ ไม่เพิ่มตัวเองใน Startup การเชื่อมต่อเครือข่ายเกิดเมื่อผู้ใช้สั่งตรวจ/ดาวน์โหลดอัปเดตจาก GitHub เท่านั้น

## Build and test

Use Rust supporting edition 2024 and a Windows linker: Visual Studio Build Tools with Desktop development with C++, or a complete MinGW-w64 toolchain.

```powershell
cargo fmt --check
cargo test --locked
powershell -NoProfile -ExecutionPolicy Bypass -File tests/updater.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tests/update-install.ps1
cargo build --release --locked
```

The binary is `target\release\mickey.exe`. The installer integration tests use temporary fixture programs; they do not change your microphone or running MicKey instance.

```powershell
.\MicKey.exe --smoke-test  # Native close-to-tray test, with taskbar fallback allowed
.\MicKey.exe --tray-test   # Requires a real working Windows system tray
.\MicKey.exe --status      # Read the default input's mute state only
```

## Publish the next version

1. Change the version in `Cargo.toml`, run `cargo check` to refresh `Cargo.lock`, and update `CHANGELOG.md`.
2. Commit the changes to `main`.
3. Create a GitHub Release with a matching tag, e.g. `v1.3.1`, targeting that commit. Alternatively push the matching tag.
4. The **Release Windows** workflow runs tests, builds on Windows, and attaches `MicKey.exe`, `MicKey-windows-x86_64.zip`, and `SHA256SUMS.txt`.
5. Existing users can select **ตรวจสอบอัปเดต…** to receive the new stable version.

CI uses GitHub's built-in token; no personal access token needs to be added. Fork builds can set `MICKEY_REPOSITORY=owner/repository` at compile time to use their own releases.

## License

[MIT](LICENSE). Copyright (c) 2026 Samsxx001.
