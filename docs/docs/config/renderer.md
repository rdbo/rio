---
title: 'renderer'
language: 'en'
---

- `Performance` - Set WGPU rendering performance

  - `High`: Adapter that has the highest performance. This is often a discrete GPU.
  - `Low`: Adapter that uses the least possible power. This is often an integrated GPU.

- `Backend` - Set WGPU rendering backend

  - `Automatic`: Leave Sugarloaf/WGPU to decide
  - `GL`: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
  - `Vulkan`: Supported on Windows, Linux/Android
  - `DX12`: Supported on Windows 10
  - `Metal`: Supported on macOS/iOS

- `disable-unfocused-render` - This property disable renderer processes while Rio is unfocused.

Example:

```toml
[renderer]
performance = "High"
backend = "Automatic"
disable-unfocused-render = false
```
