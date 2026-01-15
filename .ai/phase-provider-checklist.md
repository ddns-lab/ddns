# Phase 2: Provider 通用实现 Checklist

**版本**: v1.0
**基于**: Cloudflare Provider 实现
**适用**: 所有 DNS Provider 实现

**⚠️ 强制要求**: 所有 Provider 必须满足此 Checklist 的所有项目，否则实现无效。

---

## 📦 Crate 结构

### 必须的文件

```
crates/ddns-provider-{name}/
├── Cargo.toml                 # 依赖配置
├── src/
│   └── lib.rs                 # 主实现文件
└── tests/                     # 测试目录
    └── integration_test.rs    # 集成测试
```

### Cargo.toml 依赖要求

```toml
[package]
name = "ddns-provider-{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
ddns-core = { path = "../ddns-core" }
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.12", features = ["json"] }
tracing = "0.1"
tokio = { version = "1", features = ["full"] }

[dev-dependencies]
# 测试依赖
```

---

## 🔧 Provider 结构体

### 必需字段

```rust
pub struct {Name}Provider {
    /// API 认证凭证
    /// ⚠️ NEVER log this value (Debug 必须隐藏)
    api_token: String,  // 或 access_key, secret 等

    /// Zone ID (可选, 可自动检测)
    zone_id: Option<String>,

    /// HTTP 客户端
    /// ⚠️ 必须配置 timeout (30秒)
    client: reqwest::Client,

    /// Dry-run 模式
    /// ✅ true 时执行 GET 但跳过 PUT
    dry_run: bool,
}
```

### 必需的构造函数

```rust
impl {Name}Provider {
    /// 创建 Provider
    pub fn new(
        api_token: impl Into<String>,
        zone_id: Option<String>,
        dry_run: bool,
    ) -> Self {
        // 1. 构建 HTTP client (必须有 timeout)
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        let api_token = api_token.into();

        // 2. 验证 token 不为空
        if api_token.is_empty() {
            panic!("Provider API token cannot be empty");
        }

        Self { api_token, zone_id, client, dry_run }
    }

    /// 创建 live Provider (便捷方法)
    pub fn new_live(...) -> Self {
        Self::new(..., false)
    }

    /// 创建 dry-run Provider (便捷方法)
    pub fn new_dry_run(...) -> Self {
        Self::new(..., true)
    }
}
```

### Debug 实现 (强制)

```rust
impl std::fmt::Debug for {Name}Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("{Name}Provider")
            .field("api_token", &"<REDACTED>")  // ⚠️ 必须 REDACTED
            .field("zone_id", &self.zone_id)
            .field("dry_run", &self.dry_run)
            .finish()
    }
}
```

---

## 🎯 DnsProvider Trait 实现

### update_record() 完整逻辑步骤

```rust
#[async_trait]
impl DnsProvider for {Name}Provider {
    async fn update_record(&self, record_name: &str, new_ip: IpAddr) -> Result<UpdateResult> {
        // Step 1: 确定记录类型 (A 或 AAAA)
        let record_type = match new_ip {
            IpAddr::V4(_) => "A",
            IpAddr::V6(_) => "AAAA",
        };

        tracing::info!("Updating {} DNS record: {} -> {} [mode: {}]",
            self.provider_name(),
            record_name,
            new_ip,
            if self.dry_run { "DRY-RUN" } else { "LIVE" }
        );

        // Step 2: 获取 zone ID
        let zone_id = self.get_zone_id(record_name).await?;

        // Step 3: 获取/创建 record ID
        let (record_id, is_newly_created) =
            match self.get_record_id(&zone_id, record_name, record_type).await {
                Ok(id) => (id, false),
                Err(Error::NotFound { .. }) => {
                    tracing::info!("DNS record does not exist, creating: {}", record_name);
                    (self.create_record(&zone_id, record_name, record_type, new_ip).await?, true)
                }
                Err(e) => return Err(e),
            };

        // Step 4: 如果刚创建,返回 Created
        if is_newly_created {
            return Ok(UpdateResult::Created { new_ip });
        }

        // Step 5: 获取当前记录,检查 IP 是否相同
        let current_ip = self.get_current_record(&zone_id, &record_id).await?;

        // Step 6: 如果 IP 相同,返回 Unchanged (幂等性)
        if current_ip == new_ip {
            tracing::info!("DNS record already has correct IP: {} -> {}", record_name, new_ip);
            return Ok(UpdateResult::Unchanged { current_ip });
        }

        // Step 7: Dry-run 模式检查
        if self.dry_run {
            tracing::info!("[DRY-RUN] Would update {} DNS record: {} -> {} (was: {})",
                self.provider_name(), record_name, new_ip, current_ip);
            return Ok(UpdateResult::Updated {
                previous_ip: Some(current_ip),
                new_ip,
            });
        }

        // Step 8: 执行实际更新
        self.update_record_ip(&zone_id, &record_id, record_name, record_type, new_ip, current_ip).await?;

        Ok(UpdateResult::Updated {
            previous_ip: Some(current_ip),
            new_ip,
        })
    }

    // 其他 trait 方法...
    fn supports_record(&self, record_name: &str) -> bool {
        record_name.contains('.') && record_name.len() <= 253
    }

    fn provider_name(&self) -> &'static str {
        "{provider_name}"  // 小写, 如 "aliyun", "namesilo"
    }
}
```

---

## 🌐 HTTP 请求模式

### GET 请求模式 (查询 Zone/Record)

```rust
async fn http_get(&self, url: &str) -> Result<Value> {
    let response = self.client
        .get(url)
        .header("Authorization", self.build_auth_header()?)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| Error::provider(self.provider_name(), format!("HTTP request failed: {}", e)))?;

    // 错误映射
    if !response.status().is_success() {
        return Err(self.map_http_error(response.status(), response).await?);
    }

    // 解析 JSON
    response.json().await
        .map_err(|e| Error::provider(self.provider_name(), format!("Failed to parse response: {}", e)))
}
```

### PUT 请求模式 (更新 Record)

```rust
async fn http_put(&self, url: &str, payload: &Value) -> Result<Value> {
    let response = self.client
        .put(url)
        .header("Authorization", self.build_auth_header()?)
        .header("Content-Type", "application/json")
        .json(payload)
        .send()
        .await
        .map_err(|e| Error::provider(self.provider_name(), format!("HTTP request failed: {}", e)))?;

    // 错误映射
    if !response.status().is_success() {
        return Err(self.map_http_error(response.status(), response).await?);
    }

    response.json().await
        .map_err(|e| Error::provider(self.provider_name(), format!("Failed to parse response: {}", e)))
}
```

---

## 🔴 错误映射规则 (强制)

### HTTP 状态码映射

```rust
fn map_http_error(&self, status: StatusCode, response: Response) -> Result<!> {
    let error_text = response.text().await
        .unwrap_or_else(|_| "Unable to read error response".to_string());

    match status.as_u16() {
        // 认证/权限错误 - 不可重试
        401 | 403 => Err(Error::provider(
            self.provider_name(),
            format!("Authentication failed: Invalid API token or insufficient permissions. Status: {}", status),
        )),

        // 资源不存在 - 不可重试
        404 => Err(Error::not_found(format!("Resource not found"))),

        // 限流 - 可重试 (由 engine 决定)
        429 => Err(Error::provider(
            self.provider_name(),
            format!("Rate limit exceeded. Please retry later. Status: {}", status),
        )),

        // 服务器错误 - 可重试 (5xx)
        500..=599 => Err(Error::provider(
            self.provider_name(),
            format!("Server error (transient): {} - {}", status, error_text),
        )),

        // 其他错误 - 不可重试
        _ => Err(Error::provider(
            self.provider_name(),
            format!("Request failed: {} - {}", status, error_text),
        )),
    }
}
```

### ⚠️ 关键原则

1. **401/403** → 永久错误,不重试
2. **404** → 资源不存在,不重试
3. **429** → 限流,可重试 (由 engine 处理)
4. **5xx** → 临时错误,可重试 (由 engine 处理)
5. **其他** → 具体分析

---

## ✅ 幂等性保证方式

### 必须实现的检查

```rust
// 1. 获取当前记录
let current_ip = self.get_current_record(&zone_id, &record_id).await?;

// 2. 比较 IP
if current_ip == new_ip {
    return Ok(UpdateResult::Unchanged { current_ip });
}

// 3. 只在 IP 不同时更新
self.update_record_ip(...).await?;
```

### ⚠️ 禁止的行为

```rust
// ❌ 错误: 直接更新,不检查
self.update_record_ip(new_ip).await?;

// ❌ 错误: 在 Provider 中实现重试
loop {
    match self.update_record_ip(...).await {
        Ok(_) => break,
        Err(_) => continue,  // 禁止! 由 engine 处理
    }
}
```

---

## 🧪 测试要求 (强制)

### 单元测试 (必须)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        let factory = {Name}Factory;
        let config = ProviderConfig::{Name} { api_token: "test".into(), zone_id: None };
        assert!(factory.create(&config).is_ok());
    }

    #[test]
    fn test_factory_missing_token() {
        let factory = {Name}Factory;
        let config = ProviderConfig::{Name} { api_token: "".into(), zone_id: None };
        assert!(factory.create(&config).is_err());
    }

    #[test]
    #[should_panic(expected = "API token cannot be empty")]
    fn test_empty_token_panics() {
        {Name}Provider::new("", None, false);
    }

    #[test]
    fn test_dry_run_mode() {
        let provider_dry = {Name}Provider::new_dry_run("token", None);
        let provider_live = {Name}Provider::new_live("token", None);
        assert!(provider_dry.dry_run);
        assert!(!provider_live.dry_run);
    }

    #[test]
    fn test_api_token_not_exposed_in_debug() {
        let provider = {Name}Provider::new("secret_token", None, false);
        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret_token"));
    }

    #[test]
    fn test_supports_record() {
        let provider = {Name}Provider::new("token", None, false);
        assert!(provider.supports_record("example.com"));
        assert!(!provider.supports_record(""));
    }

    #[test]
    fn test_provider_name() {
        let provider = {Name}Provider::new("token", None, false);
        assert_eq!(provider.provider_name(), "{provider_name}");
    }
}
```

### 集成测试 (必须)

```rust
// 使用 mockito 或 wiremock 进行 mock HTTP 测试
#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::MockServer;
    use wiremock::matchers::{method, path};

    #[tokio::test]
    async fn test_update_record_success() {
        let mock_server = MockServer::start().await;

        // Mock GET current record
        Mock::given()
            .method(GET)
            .path("/records/example.com")
            .return_status(200)
            .return_body(r#"{"content": "1.2.3.4"}"#)
            .mount(&mock_server)
            .await;

        // Mock PUT update
        Mock::given()
            .method(PUT)
            .path("/records/12345")
            .return_status(200)
            .mount(&mock_server)
            .await;

        let provider = {Name}Provider::new_live("token", None);
        let result = provider.update_record("example.com", "5.6.7.8".parse().unwrap()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_record_auth_failure() {
        let mock_server = MockServer::start().await;

        Mock::given()
            .method(GET)
            .return_status(403)
            .mount(&mock_server)
            .await;

        let provider = {Name}Provider::new_live("invalid_token", None);
        let result = provider.update_record("example.com", "5.6.7.8".parse().unwrap()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_idempotent_no_change() {
        let mock_server = MockServer::start().await;

        Mock::given()
            .method(GET)
            .return_status(200)
            .return_body(r#"{"content": "1.2.3.4"}"#)
            .mount(&mock_server)
            .await;

        let provider = {Name}Provider::new_live("token", None);
        let result = provider.update_record("example.com", "1.2.3.4".parse().unwrap()).await;

        assert!(matches!(result.unwrap(), UpdateResult::Unchanged { ... }));
    }
}
```

---

## 🏭 Factory 实现

```rust
pub struct {Name}Factory;

impl DnsProviderFactory for {Name}Factory {
    fn create(&self, config: &ProviderConfig) -> Result<Box<dyn DnsProvider>> {
        match config {
            ProviderConfig::{Name} { api_token, zone_id } => {
                if api_token.is_empty() {
                    return Err(Error::config("{Provider} API token is required"));
                }

                let dry_run = std::env::var("DDNS_MODE")
                    .unwrap_or_default()
                    .to_lowercase() == "dry-run";

                if dry_run {
                    tracing::warn!("{} provider running in DRY-RUN mode", self.provider_name());
                }

                Ok(Box::new({Name}Provider::new(api_token.clone(), zone_id.clone(), dry_run)))
            }
            _ => Err(Error::config("Invalid config for {Provider} provider")),
        }
    }
}
```

---

## 📋 注册函数

```rust
/// Register the {Provider} provider with a registry
pub fn register(registry: &ddns_core::ProviderRegistry) {
    registry.register_provider("{provider_name}", Box::new({Name}Factory));
}
```

---

## ✅ 实现完整性 Checklist

### 结构要求 (必须全部满足)

- [ ] Provider 结构体包含 `api_token`, `zone_id`, `client`, `dry_run`
- [ ] HTTP client 配置了 30 秒 timeout
- [ ] Debug 实现 REDACTED 了 api_token
- [ ] 构造函数验证 token 不为空
- [ ] 提供 `new_live()` 和 `new_dry_run()` 便捷方法

### Trait 实现要求 (必须全部满足)

- [ ] `update_record()` 实现了完整的 8 个步骤
- [ ] `supports_record()` 有合理验证
- [ ] `provider_name()` 返回小写 provider 名称
- [ ] 所有错误正确映射到 `Error::provider()`

### HTTP 请求要求 (必须全部满足)

- [ ] GET/PUT 请求遵循标准模式
- [ ] 认证头正确构建
- [ ] HTTP 超时配置为 30 秒
- [ ] 错误映射遵循 5 类规则 (401/403, 404, 429, 5xx, 其他)

### 幂等性要求 (必须全部满足)

- [ ] 更新前检查当前 IP
- [ ] IP 相同时返回 `Unchanged`
- [ ] **不**在 Provider 中实现重试逻辑
- [ ] **不**在 Provider 中实现缓存

### 测试要求 (必须全部满足)

- [ ] 至少 7 个单元测试通过
- [ ] 至少 3 个集成测试通过 (mock HTTP)
- [ ] 测试覆盖: factory, token 验证, dry-run, Debug, supports_record
- [ ] 测试覆盖: 成功更新, 认证失败, 幂等性

### 安全要求 (必须全部满足)

- [ ] API token **绝不**出现在日志中
- [ ] API token **绝不**出现在错误消息中
- [ ] Debug 实现 **绝不**暴露 token
- [ ] 单元测试验证 token 不暴露

### 架构要求 (必须全部满足)

- [ ] **不**修改 `ddns-core` 的 public API
- [ ] **不**引入 provider 特有逻辑到 engine
- [ ] **不**实现重试/缓存/后台任务
- [ ] **不**跨 provider 共享状态
- [ ] 通过 ProviderRegistry 注册

---

## 🚫 禁止模式 (违反即失败)

### ❌ 禁止: 在 Provider 中重试

```rust
// ❌ 错误示例
for attempt in 0..3 {
    match self.update_record(...).await {
        Ok(_) => return Ok(()),
        Err(_) if attempt < 2 => continue,
        Err(e) => return Err(e),
    }
}

// ✅ 正确: 让 engine 处理重试
self.update_record(...).await?
```

### ❌ 禁止: 在 Provider 中缓存状态

```rust
// ❌ 错误示例
pub struct Provider {
    cache: HashMap<String, IpAddr>,  // 禁止!
}

// ✅ 正确: StateStore 管理状态
// Provider 是无状态的
```

### ❌ 禁止: 跨 Provider 共享状态

```rust
// ❌ 错误示例
static GLOBAL_CLIENT: Lazy<HttpClient> = ...;  // 禁止!

// ✅ 正确: 每个 Provider 独立
pub struct Provider {
    client: reqwest::Client,  // 独立实例
}
```

### ❌ 禁止: 打印 secret

```rust
// ❌ 错误示例
tracing::info!("Using token: {}", self.api_token);  // 禁止!

// ✅ 正确: 使用 REDACTED
tracing::info!("Using token: <REDACTED>");
```

---

## 📊 验收标准

### Phase 3 验收 (Aliyun)

- [ ] ✅ 代码能编译
- [ ] ✅ 所有单元测试通过
- [ ] ✅ 所有集成测试通过 (mock)
- [ ] ✅ dry-run 模式不执行实际更新
- [ ] ✅ Debug 不暴露 token
- [ ] ✅ 可以通过 registry 创建
- [ ] ✅ 错误正确映射到 engine

### Phase 4 验收 (Dry-Run)

- [ ] ✅ Dry-run 输出清晰的日志
- [ ] ✅ Dry-run **不**修改 DNS
- [ ] ✅ Live 模式成功更新测试域名
- [ ] ✅ 幂等性验证通过 (相同 IP 不更新)

---

## 🎯 下一步

Phase 3: 实现 Aliyun DNS Provider

使用此 Checklist 作为实现标准,确保所有项目都已完成。
