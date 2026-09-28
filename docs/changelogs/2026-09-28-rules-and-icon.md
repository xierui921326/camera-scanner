# 新增 Cursor 开发规范与应用图标

## 日期

2026-09-28

## 类型

docs / chore

## 摘要

按 privacy-photo-scanner 的规则文档风格，为本仓库补充 alwaysApply 的 Cursor 开发规范；同时生成并写入桌面应用图标资源。

## 影响范围

- 模块：规范文档、应用图标、变更留痕
- 文件：
  - `.cursor/rules/camera-scanner.mdc`
  - `src-tauri/app-icon.png`
  - `src-tauri/icons/*`（由 `pnpm tauri icon` 生成）
  - `docs/CHANGELOGS.md`
  - `docs/changelogs/_TEMPLATE.md`
  - `docs/changelogs/2026-09-28-rules-and-icon.md`
- 用户可见行为：打包后的桌面图标更新；Agent 开发约束生效

## 风险与回滚注意

- 图标替换不影响业务逻辑；若不满意可重新提供主图后执行 `pnpm tauri icon src-tauri/app-icon.png`
- 规则文档强调授权边界与无攻击载荷，后续功能不得突破

## 测试与验证

- `pnpm tauri icon` 成功生成 icns/ico/png
- 规则文件 frontmatter：`alwaysApply: true`

## 关联

- 参考：`privacy-photo-scanner/.cursor/rules/privacy-scanner.mdc`
- 架构：`docs/技术架构与可行性.md`
