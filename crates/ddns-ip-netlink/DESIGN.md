# Netlink IP Source - 设计分析与改进方案

## 问题分析

### 1. 核心设计问题

#### 问题1.1: IP查询的顺序不确定性

**现状**：
```rust
let any_v4 = addrs.iter().find(|ip| ip.is_ipv4()).copied();
let public_v4 = addrs.iter().find(|ip| ip.is_ipv4() && is_public_ip(ip)).copied();
```

**问题**：
- 当多个接口有IP时，`.find()` 返回第一个匹配的，顺序不确定
- eth0: 172.31.6.210 (private)
- veth0: 198.51.100.1 (public)
- `any_v4` 可能返回 172.31.6.210，也可能返回 198.51.100.1

**影响**：
- 日志显示的 "any" IP 可能来自不同接口，导致误导性的变化日志
- 例如：初始化时 `any_v4 = 172.31.6.210`，事件后 `any_v4 = 198.51.100.1`，但实际两个IP都存在

**解决方案**：
- 方案A：记录每个接口的IP，日志显示 "eth0: 172.31.6.210, veth0: 198.51.100.1"
- 方案B：按优先级排序接口（例如：按名称、按是否为虚拟接口）
- 方案C：只关注"第一个public IP"，不记录"any IP"

**选择**：方案A（最清晰）+ 方案C（简化）

#### 问题1.2: 状态跟踪不明确

**现状**：
- `last_any_v4` - 跟踪第一个IPv4（可能来自任何接口）
- `last_public_v4` - 跟踪第一个public IPv4（可能来自任何接口）

**问题**：
- 两个变量都可能是不同接口的IP
- 添加/删除接口时，可能两者都变化

**解决方案**：
明确区分两种使用场景：
1. **监控所有IP变化**：记录每个接口的IP列表
2. **触发DNS更新**：只跟踪全局第一个public IP

## 改进方案

### 2. 日志增强

#### 2.1 初始化日志

```rust
// 详细的初始化日志
tracing::info!("=== Netlink IP Monitoring Started ===");
tracing::info!("Interface filter: {:?}", interface); // None = all interfaces
tracing::info!("IP version filter: {:?}", version); // None = both v4 and v6
tracing::info!("Querying initial addresses...");

for iface in all_interfaces {
    let ips = get_ips_for_interface(iface);
    tracing::info!("  {}: {:?}", iface, ips);
}

let (public_v4, public_v6) = find_public_ips();
tracing::info!("Initial public IPs:");
tracing::info!("  IPv4: {:?} (for DNS updates)", public_v4);
tracing::info!("  IPv6: {:?} (for DNS updates)", public_v6);
```

#### 2.2 Netlink事件日志

```rust
// 当收到 netlink 消息时
tracing::debug!("Received netlink message: type={}, len={}, flags={}",
    msg_type, nread, flags);

// 解析出interface name和address
tracing::info!("Address event on interface '{}': {}",
    ifname, msg_type == RTM_NEWADDR ? "NEWADDR" : "DELADDR");
```

#### 2.3 变化检测日志

```rust
// 检测到public IP变化时
if last_public_v4 != new_public_v4 {
    tracing::info!("=== Public IPv4 Change Detected ===");
    tracing::info!("  Previous: {:?}", last_public_v4);
    tracing::info!("  Current:  {:?}", new_public_v4);
    tracing::info!("  Action: Sending IpChangeEvent to stream");
}
```

### 3. 异常处理

#### 3.1 Socket创建失败

```rust
let sock = match Socket::new(libc::NETLINK_ROUTE as isize) {
    Ok(s) => s,
    Err(e) => {
        tracing::error!("Failed to create netlink socket: {}", e);
        tracing::error!("Possible causes:");
        tracing::error!("  - Insufficient permissions (need CAP_NET_ADMIN)");
        tracing::error!("  - Netlink not available in this environment");
        // 返回错误给调用者
        let _ = tx.send(IpChangeEvent::error("netlink socket creation failed"));
        return;
    }
};
```

#### 3.2 查询失败

```rust
match temp_source.query_addresses_proc() {
    Ok(addrs) => {
        // 处理地址
    }
    Err(e) => {
        tracing::warn!("Failed to query addresses after netlink event: {}", e);
        tracing::warn!("This may be transient, continuing to monitor...");
        // 不要break，继续监听
        continue;
    }
}
```

### 4. 测试改进

#### 4.1 测试前清理

```bash
# 测试前清理所有测试接口
for iface in $(ip link show | grep -E '^v[0-9]|veth-' | awk '{print $2}' | tr -d ':'); do
    ip link del $iface
done
```

#### 4.2 测试步骤

```bash
# 1. 确认环境干净
ip addr show | grep -E '198.51.100|203.0.113' || echo "No test IPs found"

# 2. 启动监控程序
RUST_LOG=info ./test-netlink-events &

# 3. 创建新接口
ip link add v-test type veth peer name v-test-peer
ip link set v-test up

# 4. 添加public IP（应触发事件）
ip addr add 198.51.100.1/32 dev v-test

# 5. 等待并检查日志
sleep 2

# 6. 更换IP（应触发事件）
ip addr change 203.0.113.1/32 dev v-test

# 7. 清理
ip link del v-test
```

## 实现优先级

### Phase 1: 日志增强（必须）
- [ ] 详细的初始化日志
- [ ] Netlink消息解析日志
- [ ] 地址查询结果日志
- [ ] 变化检测的详细日志

### Phase 2: 异常处理（必须）
- [ ] Socket失败的详细错误信息
- [ ] 查询失败的警告（不中断）
- [ ] 发送失败的错误处理

### Phase 3: 测试改进（必须）
- [ ] 测试前自动清理
- [ ] 测试脚本完善
- [ ] 预期结果验证

### Phase 4: 代码优化（可选）
- [ ] 接口优先级排序
- [ ] 性能优化
- [ ] 单元测试

## 当前实现的关键缺陷

1. **缺少DEBUG级别的详细日志**
   - 不知道netlink消息来自哪个接口
   - 不知道查询到了哪些IP

2. **缺少错误恢复**
   - 一次查询失败就停止监控
   - 应该重试或至少继续监听

3. **日志信息不完整**
   - 只记录 "IPv4 any changed"
   - 应该记录具体是哪个接口的IP变化

4. **测试环境混乱**
   - 多个测试接口残留
   - 导致结果不可预测
