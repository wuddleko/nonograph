<div align="center">
  <img align="center" width="210" height="120" alt="Nostorgraph" src="logo-proposals/logo-ntor-bw.png" />
</div>

<h1 align="center">Nostorgraph</h1>

<p align="center"><b>面向重视共同隐私的网络，提供匿名、去中心化的发布。</b></p>
<div align="center">
  <a href="https://unlicense.org">
    <img alt="GitHub License" src="https://img.shields.io/github/license/wuddleko/nostorgraph">
  </a>
  <a href="https://github.com/wuddleko/nostorgraph/releases/latest">
    <img alt="GitHub Release" src="https://img.shields.io/github/v/release/wuddleko/nostorgraph">
  </a>
  <a href="https://github.com/wuddleko/nostorgraph/commits/main/">
    <img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/wuddleko/nostorgraph">
  </a>
  <a href="http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/">
    <img src="https://img.shields.io/badge/Tor-Hidden%20Service-7d4698?style=flat&logo=torproject&logoColor=white" alt="Tor Hidden Service">
  </a>

[English](README.md) | 简体中文 | [Français](README.fr.md)

</div>

这是 [du82](https://github.com/du82) 的 [nonograph](https://github.com/du82/nonograph) 的一个分支。

Nostorgraph 是一个简单的匿名发布平台，适合想把文字放到网上、却不想带上身份、也不想让单一实体持有自己想法的人。

你写下内容，点击存储，笔记被加密，你得到一个链接。别人打开这个链接，由他们的浏览器解密。

实例通过 Tor 把加密后的笔记存在 [nostr 中继](https://nostr.com/info)上。因此中继看到的是来自 Tor 出口的密文。

没有账号，没有个人主页，也不会有分析工具跟着读者和作者。

https://github.com/user-attachments/assets/d662c9a2-f0ed-4266-bf55-e2c1f024269e

## 实例

| 运行状态                                                                             | 位置     | 公网                             | Onion                                                                  |   |
|--------------------------------------------------------------------------------------|----------|----------------------------------|------------------------------------------------------------------------|---|
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnonogra.ph)              | 🏴‍☠️ 未知 | https://nonogra.ph               | http://ortmy3ey5usdzf4ivht6axtb72owjniaeqrexknosyons544aooltzyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fwrite.eversiege.network) | 🏴‍☠️ 未知 | https://write.eversiege.network/ | http://fmoigm7j3z6vh4hgssdfhlt6knkp443thgxpe5wmbaevvb5km2d3suyd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fnostorgraph.com)           | 🇭🇰 香港 | https://nostorgraph.com            | http://gt65bmujun7alps7b7oar5x2u5lprxjdvpwrxcrnwkgheocjwllchhqd.onion/ |   |
| ![Website](https://img.shields.io/website?url=https%3A%2F%2Fproxy.write.daun.world)  | 🇫🇮 芬兰 | https://proxy.write.daun.world/  | 见上                                                                   |   |
| Onion                                                                                | 🇷🇺 俄罗斯 |                                  | http://q2w7sdjlmfc5vjif6y3372665wzyqskqvjji7bmhfn72orxcbljvonid.onion/ |   |
| Onion                                                                                | 🇭🇺 匈牙利 |                                  | http://t7fgh7qvjysh3wer747m6dkjvkjsqvajyv5bh2grzjgpd2derxsxbdad.onion/ |   |
| Onion                                                                                | 🏴‍☠️ 未知 |                                  | http://uawaa47jvsfr3ij63ns25xp6qvhqswsx3fgij2evbrcnt3ygxq3dbwyd.onion/ |   |
| Onion                                                                                | 🇰🇿 哈萨克斯坦 |                              | http://5mq3db45agipsceghnpx3iumlctya3absmp4sgnitqcmrmhaqhbbjcid.onion/ |   |

这些实例由别人运营，各有自己的规则。请使用你信任的，或自行托管（[部署](#部署)）。

## 部署

`~/nostorgraph/content` 存放四个站点页面（关于、法律、接口、标记）的可选副本，以及爬取到的中继池 `relay-pool.json`。已发布的笔记不存在这里。Tor 密钥在 `~/nostorgraph/onion`。Tor 状态（包括入口守卫）在 `~/nostorgraph/tor`。出站中继流量使用 `Config.toml` 里的 `[nostr].socks`（Docker 中的 Tor 是 `127.0.0.1:9050`）。`[nostr].relays` 列出不写进分享链接的中继。

### Docker 镜像（快速）

```bash
mkdir -p ~/nostorgraph/content ~/nostorgraph/tor ~/nostorgraph/onion
curl -fsSL -o ~/nostorgraph/Config.toml \
  https://raw.githubusercontent.com/wuddleko/nostorgraph/main/Config.toml
# 编辑 ~/nostorgraph/Config.toml — [nostr].socks 应为本地 Tor 端口

sudo docker run -d \
  --name nostorgraph \
  --read-only \
  --cap-drop ALL \
  --cap-add SETUID --cap-add SETGID --cap-add CHOWN \
  --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --security-opt no-new-privileges:true \
  --tmpfs /tmp --tmpfs /run \
  -p 127.0.0.1:8009:8009 \
  -v ~/nostorgraph/content:/app/content \
  -v ~/nostorgraph/tor:/var/lib/tor \
  -v ~/nostorgraph/onion:/var/lib/tor/hidden_service \
  -v ~/nostorgraph/Config.toml:/app/Config.toml:ro \
  --restart unless-stopped \
  ghcr.io/wuddleko/nostorgraph:latest
```

公网：`http://localhost:8009`（生产环境请在前面加反向代理以提供 HTTPS）。发布的端口只监听主机的回环地址。

Onion 地址：

```bash
docker logs nostorgraph 2>&1 | tail -20
# 或容器启动后：
docker exec -u debian-tor nostorgraph cat /var/lib/tor/hidden_service/hostname
```

### 从源码构建（构建 + Compose）

```bash
git clone https://github.com/wuddleko/nostorgraph
cd nostorgraph
# 编辑 Config.toml，然后：
make up
```

`make up` 使用 `docker-compose.yml` 构建镜像并启动容器 `nostorgraph_app`。常用命令：`make logs`、`make status`、`make onion`、`make down`。

### 不用 Docker

在 Debian 系系统上，`./scripts/run` 会构建二进制文件，并安装一个以 `nostorgraph` 用户运行的 systemd 服务。服务辅助脚本：`./scripts/nostorgraph.sh`、`./scripts/status.sh`。

更多说明：[DOCKER.md](DOCKER.md)。

## 功能
Nostorgraph 带有大量标记选项；输入 `/` 即可列出。

<img width="561" height="447" alt="image" src="https://github.com/user-attachments/assets/cda96a9c-08bc-4add-bf5b-e8eb0b352201" />


## 截图
首页与写作区：

<img width="1920" height="1080" alt="homepage" src="https://github.com/user-attachments/assets/d77c065d-a02f-40f5-b29f-fe45465af018" />

编辑器中正在写的一页：

<img width="1920" height="1080" alt="editor" src="https://github.com/user-attachments/assets/7546a84f-b172-4c0d-a20a-df6c2defbf3c" />

带图片的已发布页面：

<img width="1920" height="1080" alt="published-page2" src="https://github.com/user-attachments/assets/0fc38a43-8bcc-4fbf-9087-ea4100be3e6c" />



## 名称
`anonymous` + `monograph` + `telegraph` = `nonograph`  

`nonograph` + `tor` + `nostr` = `nostorgraph`

## 许可
公有领域（[Unlicense](https://unlicense.org)）。本软件属于所有人。可使用、修改、分享，不受限制。无需署名，没有附加条件，也不提供任何担保。
