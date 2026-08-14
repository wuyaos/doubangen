# doubangen · 豆瓣简介生成器

一个用 [egui](https://github.com/emilk/egui) 构建的桌面 GUI 工具，聚合 **豆瓣（DoubanInfo）** 与 **TMDB** 两个数据源，一键生成影视条目的 BBCode / JSON 简介。豆瓣优先合并，TMDB 补充评分、多语言别名与演职员代表作。

## 功能

- **四源模式**：`豆瓣` / `TMDB` / `自动` / `聚合(merge)`
- **多输入形态**：片名、豆瓣 ID / 链接、IMDb (`tt…`)、TMDB URL
- **海报墙**：Emby 风格搜索结果，支持按「动漫 / 剧集 / 电影」分类、年份、关键词过滤
- **聚合输出**：豆瓣评分 + IMDb 评分 + TMDB 评分，演职员头像与代表作
- **历史记录**：右下角可折叠历史，点击复用已生成内容，持久化到 `history.json`
- **跨平台**：Windows / Linux 原生窗口，静态编译，无运行时依赖
- **主题**：Catppuccin Mocha 暗色 + 琥珀色点缀，系统 CJK 字体

## 截图

> TODO：补充主界面 / 设置面板截图

## 快速开始

### 1. 配置密钥

复制配置模板并填入你自己的 API Key：

```bash
cp config.example.toml config.toml
```

| Key | 申请地址 | 说明 |
|-----|---------|------|
| `doubaninfo` | https://doubaninfo.com 用户中心 | 豆瓣数据源，仅用 TMDB 时可留空 |
| `tmdb` | https://www.themoviedb.org/settings/api | TMDB v3 API Key，仅用豆瓣时可留空 |

### 2. 运行

将编译产物 `doubangen`（Linux）/ `doubangen.exe`（Windows）与 `config.toml` 放同一目录，双击或命令行启动即可。也可在应用内「⚙ 设置」面板修改密钥与默认行为，保存即时写回 `config.toml`。

## 构建

依赖 Rust 工具链（`rustup`）。

```bash
# Linux
cargo build --release
# 产物：target/release/doubangen

# Windows（交叉编译，需 mingw-w64）
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# 产物：target/x86_64-pc-windows-gnu/release/doubangen.exe
```

Linux 编译需 X11 / Wayland 开发库，例如 Ubuntu：

```bash
sudo apt-get install -y libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev \
  libxrandr-dev libxinerama-dev libxi-dev libgl1-mesa-dev libudev-dev \
  libwayland-dev libxkbcommon-dev
```

Windows 图标通过 `build.rs` 用 `windres` 嵌入 `.exe` 资源段。

## CI

[![build](https://github.com/<owner>/doubangen/actions/workflows/build.yml/badge.svg)](https://github.com/<owner>/doubangen/actions/workflows/build.yml)

GitHub Actions 每次 push / PR 自动编译 Linux 与 Windows 产物并上传为 artifact。打 tag `v*` 会额外发布 Release。

## 项目结构

```
doubangen/
├── src/
│   ├── main.rs          # egui GUI：搜索 / 海报墙 / 结果 / 历史 / 设置
│   ├── common.rs        # HTTP 连接池、重试、国家映射、URL 工具
│   ├── doubaninfo.rs    # DoubanInfo 客户端：搜索 / 详情 / 频控
│   ├── tmdb.rs          # TMDB 客户端：搜索 / 详情 / IMDb 查找 / 演职员
│   ├── schema.rs        # BBCode 渲染、豆瓣优先 merge、空字段清理
│   └── config.rs        # TOML 配置反序列化
├── assets/              # 应用图标
├── build.rs             # Windows .exe 图标嵌入（windres + ar）
├── config.example.toml  # 配置模板（脱敏）
└── Cargo.toml
```

## 许可证

[MIT](LICENSE)
