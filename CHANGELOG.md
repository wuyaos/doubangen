# Changelog

## v0.1.0

首个发布版本。

### 功能

- **四源模式**：豆瓣 / TMDB / 自动 / 聚合(merge)，豆瓣优先合并
- **多输入形态**：片名、豆瓣 ID / 链接、IMDb (`tt…`)、TMDB URL
- **海报墙**：Emby 风格搜索结果，按「动漫 / 剧集 / 电影」分类、年份、关键词过滤
- **聚合输出**：豆瓣评分 + IMDb 评分 + TMDB 评分，演职员头像与代表作
- **历史记录**：右下角可折叠历史，点击复用，持久化 `history.json`
- **跨平台**：Windows / Linux 原生窗口，静态编译无运行时依赖
- **主题**：Catppuccin Mocha 暗色 + 琥珀色点缀，系统 CJK 字体
- **设置面板**：自适应布局，API Key / 默认行为 / 限流可配置，保存即时写回 `config.toml`

### 下载

| 平台 | 文件 |
|------|------|
| Linux x86_64 | `doubangen-0.1.0-linux-x86_64` |
| Windows x86_64 | `doubangen-0.1.0-windows-x86_64.exe` |

直接运行可执行文件，首次运行自动生成 `config.toml`（程序目录优先，只读时回退用户配置目录），在「⚙ 设置」面板填入 API Key 即可。

### 已知限制

- Windows 图标依赖 `windres`（GNU 工具链），MSVC 工具链未适配
- 中文显示依赖目标系统已安装 CJK 字体
