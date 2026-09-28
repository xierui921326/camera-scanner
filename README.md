# camera-scanner

基于 **Tauri 2.0 + React + TypeScript** 的摄像头安全审计与实时预览桌面工具。

仅用于你本人拥有或已获书面授权的设备 / 网段。默认限制 RFC1918 私有网段；首次启动需确认授权声明；扫描、审计、预览操作写入本地审计日志。

## 功能

- 设备发现：端口扫描、ONVIF WS-Discovery、mDNS
- 安全审计：HTTP 指纹、CVE 规则库比对、可选弱口令核查（可关闭 / 限速 / 命中即停）
- 实时预览：`retina` 纯 Rust fMP4/MSE、ffmpeg fMP4 HLS、MPEG-TS HLS；多路网格（最多 4 路）
- 报告：Markdown / HTML 加固报告 + 审计日志查阅

## 开发

前置：Node 22+、pnpm、Rust（stable）、[Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。预览的 `mse` / `hls` 模式需本机安装 ffmpeg；`retina` 模式不需要。

```bash
make install          # 安装前端依赖
make run              # 启动桌面开发模式 (tauri dev)
make debug            # 同 run，带 RUST_LOG=debug
make check            # 前后端静态检查
make build            # 打包桌面安装包
```

等价的 pnpm 命令：

```bash
pnpm install
pnpm tauri dev
pnpm build            # 仅构建前端
pnpm tauri build      # 打包安装包
```

`make help` 可查看全部目标。

## 文档

技术架构与可行性见 [`docs/技术架构与可行性.md`](docs/技术架构与可行性.md)。

## 免责声明

本工具仅供授权范围内的安全测试与自查加固。严禁用于未授权探测或访问，后果自负。
