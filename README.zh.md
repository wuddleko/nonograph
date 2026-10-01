<div align="center">
  <img align="center" width="96" height="96" alt="android-chrome-512x512" src="https://github.com/user-attachments/assets/9a06a3fe-46ee-422c-93ad-ce3e504603c0" />
</div>

<h1 align="center">Nonograph</h1>

<p align="center"><b>为注重隐私的网络提供匿名发布服务</b></p>
<div align="center">
  <a href="https://unlicense.org">
    <img alt="GitHub License" src="https://img.shields.io/github/license/wuddleko/nonograph">
  </a>
  <a href="https://github.com/wuddleko/nonograph/releases/latest">
    <img alt="GitHub Release" src="https://img.shields.io/github/v/release/wuddleko/nonograph">
  </a>
  <a href="https://github.com/wuddleko/nonograph/commits/main/">
    <img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/wuddleko/nonograph">
  </a>
  <a href="http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/">
    <img src="https://img.shields.io/badge/Tor-Hidden%20Service-7d4698?style=flat&logo=torproject&logoColor=white" alt="Tor Hidden Service">
  </a>

[English](README.md) | 简体中文

</div>

Nonograph 是一个简单的匿名发布平台：写好内容，拿到链接即可分享。无需注册账号，没有个人主页，也不会用分析工具跟踪读者。

https://github.com/user-attachments/assets/d662c9a2-f0ed-4266-bf55-e2c1f024269e

## Nostr（欢迎更多实例）

在编辑器中点击 **On Nostr**，笔记在浏览器内签名并发送到该实例 `Config.toml` 里配置的 relay。分享的是 Nostr 链接，而不是托管在本服务器上的页面。

我们需要更多人运行支持此功能的实例。请选择确实接受公开长文（kind 30023）的 relay；只读或失效的 relay 会让按钮看起来坏了。若你已在运营 Nonograph，请配置 `[nostr].relays` 并部署，欢迎通过 PR 把实例加入下表（公网与/或 onion）。

| 运行状态 | 位置 | 公网 | Onion |
|--------|------|------|-------|
| _暂无_ | | | |

传统的「写在别人的服务器上、拿到对方域名下的链接」模式仍在使用，见下一节。

## 已知实例

| 运行状态 | 位置 | 公网 | Onion |
|--------|------|------|-------|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonogra.ph) | 🏴‍☠️ Unknown | https://nonogra.ph | http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/ |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fwrite.eversiege.network) | 🏴‍☠️ Unknown | https://write.eversiege.network/ | http://fmoigm7j3z6vh4hgssdfhlt6knkp443thgxpe5wmbaevvb5km2d3suyd.onion/ |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonograph.com) | 🇭🇰 Hong Kong | https://nonograph.com | http://gt65bmujun7alps7b7oar5x2u5lprxjdvpwrxcrnwkgheocjwllchhqd.onion/ |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fproxy.write.daun.world) | 🇫🇮 Finland | https://proxy.write.daun.world/ | 见 write.eversiege.network |
| Onion | 🇷🇺 Russia | | http://q2w7sdjlmfc5vjif6y3372665wzyqskqvjji7bmhfn72orxcbljvonid.onion/ |
| Onion | 🇭🇺 Hungary | | http://t7fgh7qvjysh3wer747m6dkjvkjsqvajyv5bh2grzjgpd2derxsxbdad.onion/ |
| Onion | 🏴‍☠️ Unknown | | http://uawaa47jvsfr3ij63ns25xp6qvhqswsx3fgij2evbrcnt3ygxq3dbwyd.onion/ |
| Onion | 🇰🇿 Kazakhstan | | http://5mq3db45agipsceghnpx3iumlctya3absmp4sgnitqcmrmhaqhbbjcid.onion/ |

由第三方运营，规则各自不同。请选用你信任的实例，或自行托管（见[部署](#部署)）。

## 部署

已发布页面保存在 `~/nonograph/content`。Tor 密钥目录为 `~/nonograph/onion`。**On Nostr** 的默认 relay 由服务器上的 `Config.toml` 中 `[nostr].relays` 决定（访客可在浏览器中添加更多 relay，仅保存在其本机）。

### Docker 镜像（快速）

```bash
mkdir -p ~/nonograph/content ~/nonograph/onion
curl -fsSL -o ~/nonograph/Config.toml \
  https://raw.githubusercontent.com/wuddleko/nonograph/main/Config.toml
# 编辑 ~/nonograph/Config.toml — 尤其是 [nostr].relays

sudo docker run -d \
  --name nonograph \
  -p 8009:8009 \
  -v ~/nonograph/content:/app/content \
  -v ~/nonograph/onion:/var/lib/tor/hidden_service \
  -v ~/nonograph/Config.toml:/app/Config.toml:ro \
  --restart unless-stopped \
  ghcr.io/wuddleko/nonograph:latest
```

公网访问：`http://localhost:8009`（生产环境请在前面加反向代理以提供 HTTPS）。

查看 onion 地址：

```bash
docker logs nonograph 2>&1 | tail -20
# 或容器启动后：
docker exec -u debian-tor nonograph cat /var/lib/tor/hidden_service/hostname
```

### 从源码构建（Compose）

```bash
git clone https://github.com/wuddleko/nonograph
cd nonograph
# 编辑 Config.toml 后：
make up
```

`make up` 使用 `docker-compose.yml` 构建镜像并启动容器 `nonograph_app`。常用命令：`make logs`、`make status`、`make onion`、`make down`。

### 不用 Docker

在 Debian 系系统上可运行 `./scripts/run` 原生构建与启动。服务脚本：`./scripts/nonograph.sh`、`./scripts/status.sh`。

更多说明见 [DOCKER.md](DOCKER.md)。

## 功能

Nonograph 支持丰富的标记语法；在新行输入 `/` 可查看可用格式。

<img width="561" height="447" alt="image" src="https://github.com/user-attachments/assets/cda96a9c-08bc-4add-bf5b-e8eb0b352201" />

## 名称

`anonymous` + `monograph` + `telegraph` = `nonograph`

## 安全审计

- 2025-10-15 — [《安全评估报告（已编辑）.pdf》](https://github.com/user-attachments/files/27242849/Security.Assessment.Report.Redacted.pdf) — 针对 v0.0.1 的审计（门罗币支付）。完整审计条目见 [英文 README](README.md#audits-and-security)。

## 许可协议

公共领域（[Unlicense](https://unlicense.org)）。可自由使用、修改与分发，无需署名，不提供任何担保。
