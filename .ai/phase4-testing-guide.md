# Phase 4: Aliyun Provider Testing Guide

**Purpose**: Validate Aliyun provider in dry-run and live environments

**Status**: Requires Aliyun credentials to complete

---

## 🧪 Testing Requirements

### Prerequisites

1. **Aliyun Account**: Alibaba Cloud account with DNS service enabled
2. **AccessKey**: AccessKey ID and Secret with DNS permissions
3. **Test Domain**: A domain in Aliyun DNS for testing
4. **Test Records**: Subdomain records like `ddns-test.example.com`

---

## 📋 Test Scenarios

### Scenario 1: Dry-Run Mode (Safe Testing)

**Purpose**: Verify provider logic without making actual changes

```bash
# Set environment variables
export DDNS_PROVIDER_TYPE=aliyun
export DDNS_PROVIDER_ACCESS_KEY_ID=your_access_key_id
export DDNS_PROVIDER_ACCESS_KEY_SECRET=your_access_key_secret
export DDNS_RECORDS=ddns-test.example.com
export DDNS_MODE=dry-run

# Run ddnsd (will query but not update)
cargo run --bin ddnsd --features aliyun
```

**Expected Results**:
- ✅ Logs show "Updating aliyun DNS record: ddns-test.example.com -> X.X.X.X [mode: DRY-RUN]"
- ✅ Logs show "[DRY-RUN] Would update aliyun DNS record"
- ✅ No actual DNS record changes
- ✅ Returns UpdateResult::Updated without calling UpdateDomainRecord API

**Log Output Example**:
```
INFO ddnsd v0.1.0 starting
INFO Registering Aliyun provider
INFO Configuration loaded: 1 record(s), file state store
INFO Updating aliyun DNS record: ddns-test.example.com -> 192.0.2.100 [mode: DRY-RUN]
INFO [DRY-RUN] Would update aliyun DNS record: ddns-test.example.com -> 192.0.2.100 (was: 192.0.2.1)
```

---

### Scenario 2: Live Mode - New Record Creation

**Purpose**: Test auto-create functionality

```bash
# Start with no existing DNS record
export DDNS_PROVIDER_TYPE=aliyun
export DDNS_PROVIDER_ACCESS_KEY_ID=your_access_key_id
export DDNS_PROVIDER_ACCESS_KEY_SECRET=your_access_key_secret
export DDNS_RECORDS=new-record.example.com

# Run ddnsd in live mode
cargo run --bin ddnsd --features aliyun
```

**Expected Results**:
- ✅ DescribeDomainRecords API call returns empty list
- ✅ AddDomainRecord API call creates new record
- ✅ Returns UpdateResult::Created { new_ip }
- ✅ Record appears in Aliyun DNS console

**Validation**:
```bash
# Query DNS to verify record was created
dig new-record.example.com

# Check Aliyun DNS console
# Login to Aliyun → DNS → Domain → Record List
```

---

### Scenario 3: Live Mode - Update Existing Record

**Purpose**: Test standard update flow

```bash
# Start with existing DNS record pointing to old IP
export DDNS_PROVIDER_TYPE=aliyun
export DDNS_PROVIDER_ACCESS_KEY_ID=your_access_key_id
export DDNS_PROVIDER_ACCESS_KEY_SECRET=your_access_key_secret
export DDNS_RECORDS=ddns-test.example.com

# Run ddnsd
cargo run --bin ddnsd --features aliyun
```

**Expected Results**:
- ✅ DescribeDomainRecords finds record ID
- ✅ DescribeDomainRecordInfo gets current IP (different from new IP)
- ✅ UpdateDomainRecord updates the IP
- ✅ Returns UpdateResult::Updated { previous_ip: Some(old_ip), new_ip }
- ✅ DNS query shows new IP

**Validation**:
```bash
# Before: Old IP
dig ddns-test.example.com +short
# Output: 192.0.2.1

# After: New IP
dig ddns-test.example.com +short
# Output: 192.0.2.100
```

---

### Scenario 4: Idempotency (IP Unchanged)

**Purpose**: Verify no unnecessary API calls

```bash
# Set current IP as the "new" IP
export DDNS_PROVIDER_TYPE=aliyun
export DDNS_PROVIDER_ACCESS_KEY_ID=your_access_key_id
export DDNS_PROVIDER_ACCESS_KEY_SECRET=your_access_key_secret
export DDNS_RECORDS=ddns-test.example.com

# Manually set state to match current IP
# (Simulate case where IP hasn't changed)
echo '{"last_ip": "192.0.2.100", "last_update": "2025-01-15T10:00:00Z"}' > /tmp/ddns-state.json

# Run ddnsd
cargo run --bin ddnsd --features aliyun -- --state-file /tmp/ddns-state.json
```

**Expected Results**:
- ✅ DescribeDomainRecords finds record
- ✅ DescribeDomainRecordInfo gets current IP (matches new IP)
- ✅ Returns UpdateResult::Unchanged { current_ip }
- ✅ NO UpdateDomainRecord API call made
- ✅ Log: "DNS record already has correct IP"

**Log Output**:
```
INFO DNS record already has correct IP: ddns-test.example.com -> 192.0.2.100
```

---

### Scenario 5: IPv6 (AAAA Record)

**Purpose**: Test IPv6 support

```bash
export DDNS_PROVIDER_TYPE=aliyun
export DDNS_PROVIDER_ACCESS_KEY_ID=your_access_key_id
export DDNS_PROVIDER_ACCESS_KEY_SECRET=your_access_key_secret
export DDNS_RECORDS=ipv6-test.example.com:AAAA

# Run ddnsd (will monitor IPv6)
cargo run --bin ddnsd --features aliyun
```

**Expected Results**:
- ✅ Uses Type=AAAA in API calls
- ✅ Updates IPv6 address correctly
- ✅ dig AAAA query returns new IPv6

**Validation**:
```bash
dig AAAA ipv6-test.example.com +short
# Output: 2001:db8::100
```

---

### Scenario 6: Error Handling

**Purpose**: Verify error mapping and retry behavior

#### Test 6a: Invalid Credentials (403)
```bash
export DDNS_PROVIDER_ACCESS_KEY_ID=invalid_key
export DDNS_PROVIDER_ACCESS_KEY_SECRET=invalid_secret

# Run ddnsd
cargo run --bin ddnsd --features aliyun
```

**Expected Results**:
- ✅ API returns 403 Forbidden
- ✅ Returns Error::provider("aliyun", "Authentication failed...")
- ✅ Engine logs permanent error (no retry)

**Log Output**:
```
ERROR Authentication failed: Invalid API token or insufficient permissions
```

#### Test 6b: Rate Limiting (429)
```Purpose**: Trigger rate limit (hard to test without API access)

**Expected Results**:
- ✅ API returns 429 Too Many Requests
- ✅ Returns Error::provider("aliyun", "Rate limit exceeded...")
- ✅ Engine retries with backoff

#### Test 6c: Server Error (5xx)
```Purpose**: Test transient error handling

**Expected Results**:
- ✅ API returns 500 Internal Server Error
- ✅ Returns Error::provider("aliyun", "Server error (transient)...")
- ✅ Engine retries with backoff

---

### Scenario 7: Signature Validation

**Purpose**: Verify HMAC-SHA1 signature is correct

**Manual Test**:
1. Capture API request URL from logs
2. Verify signature matches Aliyun documentation
3. Check parameters are sorted alphabetically
4. Check encoding is correct (URL encoding)

**Expected Signature Format**:
```
StringToSign = "GET&%2F&" + URLencode(CanonicalizedQueryString)
Signature = HMAC-SHA1(AccessKeySecret, StringToSign)
```

---

## ✅ Phase 4 Completion Checklist

- [ ] Dry-run mode logs show correct behavior (no updates)
- [ ] New record auto-creation works
- [ ] Existing record update works
- [ ] Idempotency verified (no update when IP unchanged)
- [ ] IPv6 (AAAA) record support works
- [ ] Invalid credentials return permanent error
- [ ] Rate limiting returns retryable error
- [ ] Server errors return retryable error
- [ ] HMAC-SHA1 signature is correct
- [ ] DNS queries verify updates
- [ ] Aliyun console shows correct records

---

## 🚨 Common Issues

### Issue 1: "Authentication failed"
**Cause**: Invalid AccessKey ID or Secret
**Solution**:
1. Verify AccessKey in Aliyun console
2. Check permissions (needs Aliyun DNS full access)
3. Regenerate AccessKey if needed

### Issue 2: "Record not found" but creation fails
**Cause**: Missing AddDomainRecord permission
**Solution**: Grant "Aliyun DNS full access" to AccessKey

### Issue 3: "Signature does not match"
**Cause**: HMAC-SHA1 signature calculation error
**Solution**: This is a bug - report it if encountered

### Issue 4: Rate limiting
**Cause**: Too many API calls
**Solution**: Engine should handle retries automatically

---

## 📊 Performance Metrics

Record these metrics during testing:

- **DescribeDomainRecords latency**: ~50-200ms
- **DescribeDomainRecordInfo latency**: ~50-200ms
- **UpdateDomainRecord latency**: ~100-300ms
- **AddDomainRecord latency**: ~100-300ms
- **Total update time**: < 1 second

---

## 🔗 References

- [Aliyun DNS API Documentation](https://help.aliyun.com/zh/dns/api-alidns-2015-01-09-summary)
- [Aliyun AccessKey Management](https://ram.console.aliyun.com/manage/ak)
- [HMAC-SHA1 Signature Guide](https://help.aliyun.com/zh/dns/developer-reference/api-alidns-2015-01-09-signature)

---

## 🎯 Next Phase

After Phase 4 validation is complete:

**Phase 5**: Abstract stability verification
- Verify DnsProvider trait doesn't need changes
- Check if all providers can be implemented with current API
- Document any needed trait modifications (if any)
