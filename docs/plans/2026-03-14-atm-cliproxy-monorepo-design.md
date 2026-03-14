# ATM CLIProxy Monorepo Design

**Date:** 2026-03-14

## Goal

把 `CLIProxyAPI-wjq` 的 Go 源码完整纳入 `augment-token-mng`，让仓库本身成为唯一事实来源。

运行时仍然保持当前 sidecar 模式：

- ATM/Tauri 继续做控制平面
- `cliproxy-server` 继续做协议翻译引擎
- 用户继续只启动 ATM，不需要手工启动第二个服务

## Problem Statement

当前状态已经完成了运行层集成，但还没有完成源码层集成：

- `augment-token-mng` 负责 sidecar 生命周期、统一 `/v1` 网关、账号同步、UI 配置
- `cliproxy-server` 以预编译二进制形式打包在 ATM 中
- 真正的翻译逻辑、`previous_response_id` continuation、Auggie/OpenAI 协议适配仍然只存在于外部 Go 仓库

这导致三个实际问题：

1. 调试时要跨两个仓库来回切
2. 二进制和源码可能漂移
3. 版本发布时很难保证 ATM 内置的 sidecar 与当前源码严格一致

## Approaches Considered

### Option A: 继续维持双仓库，ATM 只引用预编译二进制

优点：

- 改动最小
- 当前流程已经能跑

缺点：

- 源码与内置二进制继续分裂
- 调试 continuation、token 统计、协议翻译时成本高
- 长期维护风险最高

### Option B: 把 Go 源码直接纳入 `augment-token-mng`，构建时自动编译 sidecar，运行时仍保持 sidecar

优点：

- 一个仓库就能同时修改 ATM 和翻译引擎
- 运行形态不变，迁移风险可控
- 便于以后给 sidecar 增加诊断日志、测试、版本锁定

缺点：

- 要整理 Go 源码目录、构建脚本、Tauri 打包链
- CI 和本地构建环境需要显式依赖 Go

### Option C: 把 Go 协议翻译逻辑全部重写进 Rust/Tauri 主进程

优点：

- 最终只有一个进程
- 部署最彻底

缺点：

- 风险最高
- 需要重写并重新验证约 12000 行翻译内核
- 会把当前可用的系统重新拉回不稳定状态

## Recommendation

采用 **Option B**。

这是当前最稳妥的“一体化”路线：

- 先把源码、构建、发布统一起来
- 保留已经验证过的 sidecar 运行模型
- 以后如果要继续消 sidecar，再从单仓库内部做第二阶段重构

## Scope

本阶段包含：

- 将 `CLIProxyAPI-wjq` 的 Go 源码纳入 `augment-token-mng`
- 让 ATM 构建流程自动编译 `cliproxy-server`
- 让打包产物继续从 ATM 仓库产出
- 保持当前 `/v1` 统一网关、sidecar 生命周期和 auth 同步逻辑不变

本阶段不包含：

- 把 Go 翻译逻辑重写成 Rust
- 改变当前 sidecar 架构
- 大规模重构 Go 内核逻辑

## Repository Layout

推荐新增如下目录：

```text
augment-token-mng/
  sidecars/
    cliproxy/
      apps/server-go/
      sdk/
      ...
  scripts/
    build-cliproxy.sh
  src-tauri/
    resources/
      cliproxy-server        # 构建产物，不再手工维护
```

设计原则：

- Go 源码直接 vendor 到当前仓库，不使用 `git submodule`
- 保留原 Go 仓库的大体目录结构，减少迁移期 diff 噪音
- sidecar 构建脚本放在 ATM 仓库统一管理

## Build Design

### Single Source Of Truth

`cliproxy-server` 不再被视为“外部下载来的预编译文件”，而是由当前仓库源码构建出来的产物。

### Build Entry

新增统一脚本，例如：

```bash
scripts/build-cliproxy.sh
```

职责：

- 检测 Go 环境
- 切换到 `sidecars/cliproxy/apps/server-go`
- 根据当前目标平台设置 `GOOS` / `GOARCH`
- 编译生成 `src-tauri/resources/cliproxy-server`

### Tauri Integration

Tauri 构建前执行 sidecar 编译，建议顺序为：

1. 本地开发：显式执行 `scripts/build-cliproxy.sh`
2. 正式构建：在 `src-tauri/build.rs` 或等价构建入口中自动执行该脚本

目标效果：

- `cargo tauri dev` / `cargo tauri build` 使用当前仓库里的 Go 源码现编 sidecar
- `src-tauri/resources/cliproxy-server` 仍然是运行时查找路径，尽量不改现有 Rust sidecar 代码

## Runtime Design

运行时行为保持不变：

1. ATM 启动
2. `AppState` 注册 `AugmentSidecar`
3. 首个 Augment `/v1/*` 请求触发懒启动
4. ATM 写 auth/config
5. ATM 启动 `cliproxy-server`
6. 请求继续转发到 sidecar

也就是说：

- **源码一体化**
- **构建一体化**
- **运行时仍双进程**

## Debugging Model

完成迁移后，排查问题只需要在一个仓库内操作：

- 查 ATM 网关和 sidecar 编排：`src-tauri/src/platforms/augment/*`
- 查翻译逻辑：`sidecars/cliproxy/apps/server-go/*`
- 查最终运行二进制：`src-tauri/resources/cliproxy-server`

这样后续分析 `previous_response_id`、上下文重放、token 统计错误时，不再需要跳回外部仓库。

## Migration Stages

### Stage 1: 纳管源码

- 把 Go 源码复制进当前仓库
- 保持目录结构尽量稳定
- 先不改业务逻辑

### Stage 2: 接入构建链

- 增加 sidecar 构建脚本
- 让 Tauri dev/build 能自动得到最新二进制

### Stage 3: 清理旧依赖

- 更新文档
- 删除“依赖外部 CLIProxyAPI 仓库手工编译”的说明
- 把生成物加入合适的忽略规则或产物规则

### Stage 4: 加强验证

- 增加 smoke test
- 验证 `/v1/models`、`/v1/responses`、`/v1/chat/completions`
- 验证打包后资源路径仍然正确

## Risks

### Risk 1: Go 构建环境差异

如果本机缺少 Go，sidecar 自动编译会失败。

缓解：

- 构建脚本输出清晰错误
- 文档显式声明 Go 版本要求

### Risk 2: 路径变更导致 Go 项目编译失败

Go 仓库中可能存在相对路径、嵌套模块或脚本假设。

缓解：

- 迁移初期尽量保留目录结构
- 先让原始测试或最小构建通过，再做整理

### Risk 3: 构建链和运行链脱钩

如果构建产物路径和 Rust 侧查找路径不一致，ATM 会找不到 sidecar。

缓解：

- 继续沿用 `src-tauri/resources/cliproxy-server`
- 保持 [sidecar.rs](/Users/jqwang/05-api-代理/augment-token-mng/src-tauri/src/platforms/augment/sidecar.rs) 和 [lib.rs](/Users/jqwang/05-api-代理/augment-token-mng/src-tauri/src/lib.rs) 的查找逻辑不变

## Success Criteria

以下条件同时满足，才算迁移完成：

1. `augment-token-mng` 仓库内包含完整 Go sidecar 源码
2. 不再需要依赖外部 `CLIProxyAPI-wjq` 仓库参与日常开发
3. 在当前仓库中执行构建后，能自动生成 `src-tauri/resources/cliproxy-server`
4. ATM 启动后仍可通过统一 `/v1` 网关访问 Augment
5. sidecar 的协议翻译行为与迁移前保持一致

## Decision

确定采用：

- **单仓库源码管理**
- **Go 源码直接 vendor 到 `augment-token-mng`**
- **构建时自动编译 sidecar**
- **运行时继续 sidecar**

这是第一阶段的一体化方案。
