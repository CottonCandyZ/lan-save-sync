# 发布 Alpha

Alpha 版本通过推送 Git tag 触发构建和发布。普通的 `main` push、Pull Request 和手动运行 CI 都不会创建 Release。

## 版本格式

Alpha tag 必须使用 UTC 时间并精确到秒：

```text
vYYYYMMDD.HHMMSS-alpha
```

例如：

```text
v20260728.050023-alpha
```

CI 会校验 tag 格式。不要复用或移动已经推送的 tag；需要修复时，使用新的 UTC 时间创建新 tag。

## 发布前检查

1. 确认需要发布的更改已经合入 `main`。
2. 确认本地工作区干净。
3. 拉取远端最新的 `main`。
4. 创建一个新的秒级时间 tag 并推送。

## Bash

```bash
git switch main
git pull --ff-only

tag="v$(date -u +'%Y%m%d.%H%M%S')-alpha"
git tag "$tag"
git push origin "$tag"
```

## PowerShell

```powershell
git switch main
git pull --ff-only

$tag = "v$((Get-Date).ToUniversalTime().ToString('yyyyMMdd.HHmmss'))-alpha"
git tag $tag
git push origin $tag
```

## 发布过程

推送符合格式的 tag 后，GitHub Actions 会：

1. 运行 Rust 格式检查、Clippy 和测试。
2. 构建 Windows Agent。
3. 构建 Steam Deck/Linux Agent。
4. 构建 Decky 插件。
5. 将构建结果打包。
6. 创建同名 GitHub prerelease 并上传产物。

只有所有测试和构建任务都成功后，Release 才会创建。

## Release 产物

每次 Alpha Release 包含：

- `lan-save-sync-windows-amd64-<tag>.zip`
- `lan-save-sync-steam-deck-amd64-<tag>.tar.gz`
- `lan-save-sync-decky-<tag>.zip`

其中 `<tag>` 是完整版本号，例如 `v20260728.050023-alpha`。

## 构建失败

在仓库的 Actions 页面打开对应 tag 的 CI 运行记录并查看失败任务。修复问题并合入 `main` 后，创建一个新的时间 tag；不要把旧 tag 移动到新提交。
