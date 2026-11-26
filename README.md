# wifi-buddy（Wi-Fi 小管家）

面向普通用户的跨平台桌面 Wi-Fi 小管家（支持 **macOS** 与 **Windows**），把晦涩的网络状态翻译成大白话和一键操作。

## 当前已实现功能（V0.1 MVP）

- **谁在用我的网（局域网设备雷达）**
  - 并发探测当前子网活跃终端，结合 ARP + 纯 Rust mDNS 反向解析 + 离线 OUI 厂商库识别设备品牌与类型。
  - 自动识别隐私随机 MAC 地址，支持给熟人设备起备注（本地持久化保存），新出现设备标黄提醒。
  - 默认隐藏 IP / MAC 等技术字段，点击开关即可展开。
- **扫码连网与密码分享**
  - 无需登录路由器，调用系统安全存储（macOS Keychain / Windows `netsh`）读取当前已保存 Wi-Fi 密码并生成标准扫码二维码。
- **一键网络体检**
  - 三段式检测“本机 ↔ 路由器”、“路由器 ↔ 外网宽带”和综合体验，用大白话告知故障原因与建议。
- **路由器指纹识别**
  - 自动识别网关品牌（如中兴 ZTE、小米、TP-Link、华为、OpenWrt 等），提供一键跳转路由器后台与三步改密指引。

## 技术栈

- **桌面壳与系统托盘**：Tauri 2.0（Rust）
- **前端界面**：React + TypeScript + Tailwind CSS
- **功能文档**：见 [docs/PRD.md](./docs/PRD.md)

## 本地开发与构建

前置依赖：Node.js 20+、Rust（`cargo`）

```bash
# 安装前端依赖
npm install

# 启动桌面开发模式（macOS / Windows）
npm run tauri dev

# 打包当前系统安装包（macOS 生成 .dmg / .app，Windows 生成 .msi / .exe）
npm run tauri build
```
