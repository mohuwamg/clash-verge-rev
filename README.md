<h1 align="center">
  <img src="./src-tauri/icons/icon.png" alt="Clash" width="128" />
  <br>
  Continuation of <a href="https://github.com/zzzgydi/clash-verge">Clash Verge</a>
  <br>
</h1>

<h3 align="center">
A Clash Meta GUI based on <a href="https://github.com/tauri-apps/tauri">Tauri</a>.
</h3>

<p align="center">
  Languages:
  <a href="./README.md">简体中文</a> ·
  <a href="./docs/README_en.md">English</a> ·
  <a href="./docs/README_es.md">Español</a> ·
  <a href="./docs/README_ru.md">Русский</a> ·
  <a href="./docs/README_ja.md">日本語</a> ·
  <a href="./docs/README_ko.md">한국어</a> ·
  <a href="./docs/README_fa.md">فارسی</a>
</p>

## Preview

| Dark                             | Light                             |
| -------------------------------- | --------------------------------- |
| ![预览](./docs/preview_dark.png) | ![预览](./docs/preview_light.png) |

## Install

请到发布页面下载对应的安装包：[Release page](https://github.com/clash-verge-rev/clash-verge-rev/releases)<br>
Go to the [Release page](https://github.com/clash-verge-rev/clash-verge-rev/releases) to download the corresponding installation package<br>
Supports Windows (x64/x86), Linux (x64/arm64) and macOS 11+ (intel/apple).

#### 我应当怎样选择发行版

| 版本        | 特征                                     | 链接                                                                                   |
| :---------- | :--------------------------------------- | :------------------------------------------------------------------------------------- |
| Stable      | 正式版，高可靠性，适合日常使用。         | [Release](https://github.com/clash-verge-rev/clash-verge-rev/releases)                 |
| Alpha(废弃) | 测试发布流程。                           | [Alpha](https://github.com/clash-verge-rev/clash-verge-rev/releases/tag/alpha)         |
| AutoBuild   | 滚动更新版，适合测试反馈，可能存在缺陷。 | [AutoBuild](https://github.com/clash-verge-rev/clash-verge-rev/releases/tag/autobuild) |

#### 安装说明和常见问题，请到 [文档页](https://clash-verge-rev.github.io/) 查看

### TG 频道: [@clash_verge_rev](https://t.me/clash_verge_re)

---

## Promotion

### ✈️ [AI云边 -- 全新架构机场 ClaudeBorder](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

🔥热销中使用本链接注册即送 3 天免费试用**，每日 **1GB 流量**：👉 [点此注册](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

#### AI云边 -- 全新架构机场。

- 💻 多次**技术迭代后**全新亮相。
- 🗺 全**高速稳定**正价节点。
- 🌏 **海外团队**，不跑路
- 🚀 线路**冗余**设计，自动化运维**对抗各类封锁
- 👨‍🦲 团队架构师为**大厂**网络架构师
- 💰 极致**稳定**，亲民价**价格**
- 🌐 全面支持**流媒体及各AI访问**
- 🙋 7*12小时真人客服。解决您的各类问题。

🌐 官网：👉 [https://www.claudeborder.com](https://cruise.54678999.xyz/#/register?code=58q5UJZc)

### 🤖 [GPTKefu —— 与 Crisp 深度整合的 AI 智能客服平台](https://gptkefu.com)

- 🧠 深度理解完整对话上下文 + 图片识别，自动给出专业、精准的回复，告别机械式客服。
- ♾️ **不限回答数量**，无额度焦虑，区别于其他按条计费的 AI 客服产品。
- 💬 售前咨询、售后服务、复杂问题解答，全场景轻松覆盖，真实用户案例已验证效果。
- ⚡ 3 分钟极速接入，零门槛上手，即刻提升客服效率与客户满意度。
- 🎁 高级套餐免费试用 14 天，先体验后付费：👉 [立即试用](https://gptkefu.com)
- 📢 智能客服TG 频道：[@crisp_ai](https://t.me/crisp_ai)

---

## Features

- 基于性能强劲的 Rust 和 Tauri 2 框架
- 内置[Clash.Meta(mihomo)](https://github.com/MetaCubeX/mihomo)内核，并支持切换 `Alpha` 版本内核。
- 简洁美观的用户界面，支持自定义主题颜色、代理组/托盘图标以及 `CSS Injection`。
- 配置文件管理和增强（Merge 和 Script），配置文件语法提示。
- 系统代理和守卫、`TUN(虚拟网卡)` 模式。
- 可视化节点和规则编辑
- WebDav 配置备份和同步

### 自动故障切换本机通知接口

代理页的“链式代理 → 多入口自动切换”支持选择多个入口候选和一个出口。实际链路为“电脑 → 一个入口 → 出口 → 网站”，入口按添加顺序自动回退，每 20 秒通过普通连通性地址检查一次。出口名称中同 IP 的主线、备用会共用这个入口组；启用 Codex 自动恢复时，受管代理组仅包含这两个出口，仍按正常 60 秒、双线失败后等待 30 秒的规则检测完整链路。入口可访问普通网络即可，不要求入口本身能访问 OpenAI。

多入口设置绑定保存时的订阅，重启和订阅刷新后会重新应用。入口与出口主备不能重叠，当前支持配置 `proxies` 中的独立节点；代理集合节点暂不支持。缺失节点或原有链路冲突会报错，不会静默改走直连。界面显示当前实际入口和出口；“配置已应用”表示内核配置已生效，目标可用性以 Codex 页检测为准。原来的多跳功能保留在“节点依次串联”中，停用多入口后可使用。

仅检查 OpenAI API 网络连通性时，可将目标网站设为 `https://api.openai.com/v1/models`。检测不发送密钥、不调用模型；该 HTTPS 接口返回结构正确的 `401` 认证错误时，显示“网络已连通（接口需要认证）”，不会因此切换节点。普通网页的 `401`、访问拒绝、限流及服务器错误不会被这个例外放行。此项只验证运行 Clash 的机器到该接口的连接，不代表 Codex 账号、模型或长连接可用。

目标网站和延迟探测地址均默认使用 `https://api.openai.com/v1/models`。延迟探测用于逐个节点测速，填写多个地址时按顺序尝试至第一个成功；目标网站则需要逐个检查。修改默认值不会覆盖已保存的配置。旧版本不支持上述 API 认证响应判定，需升级后再用该接口作为目标网站。

当前节点正常时保持不变。自动切换仅限名称中 IPv4 地址相同、以“主线”或“备用”结尾的相反线路；任何 IP 均使用相同规则。故障后直接检查同 IP 配对线路的目标网站，延迟测速只用于展示，不作为候选筛选条件。不会跨 IP 随机选择、因延迟更低迁移、恢复到其他 IP 的历史节点或因检测失败刷新订阅。无法识别当前 IP、主备标记或找不到配对线路时，不自动选择其他 IP；用户仍可手动选择新的 IP。

正常每 60 秒检测一次。当前线路及同 IP 配对线路都不可用时，本轮结束后等待 30 秒重新检测，恢复后回到 60 秒。需要人机验证时保留当前线路，按正常周期复查。节点延迟展示仍每分钟刷新，网络恢复检测不复用延迟失败缓存。

同机程序直接发送 `POST http://127.0.0.1:17897/commands/auto-failover/check`，请求头为 `x-clash-failover-notify: 1`，无请求体。所有电脑都使用相同地址和请求头，不需要查询端口或令牌。`17897` 是专用通知端口；`7897` 是 Clash 的混合代理端口，不能同时监听通知接口。

收到 `202 Accepted` 表示已触发一次检测；检测和节点切换仍遵循应用内已启用的自动故障切换配置。应用尚未准备好时返回 `503`，缺少或写错请求头时返回 `403`。若 `17897` 已被其他程序占用，通知接口无法启动，错误会写入应用日志。

```sh
curl -X POST -H 'x-clash-failover-notify: 1' 'http://127.0.0.1:17897/commands/auto-failover/check'
```

旧的实例端口接口仍可使用：

`POST http://127.0.0.1:<port>/commands/auto-failover/check`，请求头为 `x-instance-token: <token>`。

旧接口的 `port` 和 `token` 来自应用启动后写入的 `singleton-instance.json`（JSON 字段名分别为 `port`、`token`）。端口可能因占用而变化，令牌每次启动都会更新。两个接口都仅监听 `127.0.0.1`。

| 系统 | 实例文件路径 |
| --- | --- |
| macOS | `~/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/singleton-instance.json` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/io.github.clash-verge-rev.clash-verge-rev/singleton-instance.json` |
| Windows | `%APPDATA%\io.github.clash-verge-rev.clash-verge-rev\singleton-instance.json` |

便携版文件位于程序所在目录的 `.config/io.github.clash-verge-rev.clash-verge-rev/`；开发版目录名末尾增加 `.dev`。

macOS 示例（需安装 `jq`）：

```sh
record="$HOME/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/singleton-instance.json"
port=$(jq -r .port "$record")
token=$(jq -r .token "$record")
curl -X POST -H "x-instance-token: $token" "http://127.0.0.1:$port/commands/auto-failover/check"
```

### FAQ

Refer to [Doc FAQ Page](https://clash-verge-rev.github.io/faq/windows.html)

### Donation

[捐助Clash Verge Rev的开发](https://github.com/sponsors/clash-verge-rev)

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md) for more details.

To run the development server, execute the following commands after all prerequisites for **Tauri** are installed:

```shell
pnpm i
pnpm run prebuild
pnpm dev
```

`pnpm dev` preserves the Development Channel's installed service state: an
existing service is used, while a previously uninstalled service remains
uninstalled and the app starts in Sidecar mode. Use `pnpm dev:service` to
explicitly install or update the isolated development service before launch,
or `pnpm dev:sidecar` to force the unprivileged Sidecar workflow.

## Contributions

Issue and PR welcome!

## Acknowledgement

Clash Verge rev was based on or inspired by these projects and so on:

- [zzzgydi/clash-verge](https://github.com/zzzgydi/clash-verge): A Clash GUI based on tauri. Supports Windows, macOS and Linux.
- [tauri-apps/tauri](https://github.com/tauri-apps/tauri): Build smaller, faster, and more secure desktop applications with a web frontend.
- [Dreamacro/clash](https://github.com/Dreamacro/clash): A rule-based tunnel in Go.
- [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo): A rule-based tunnel in Go.
- [Fndroid/clash_for_windows_pkg](https://github.com/Fndroid/clash_for_windows_pkg): A Windows/macOS GUI based on Clash.
- [vitejs/vite](https://github.com/vitejs/vite): Next generation frontend tooling. It's fast!

## License

GPL-3.0 License. See [License here](./LICENSE) for details.
