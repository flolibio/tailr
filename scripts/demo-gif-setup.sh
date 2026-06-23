#!/bin/bash
# tailr Demo GIF 录制脚本
# 用途：生成示例日志数据，用于录制 demo GIF

set -e

# 颜色定义
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== tailr Demo GIF 录制脚本 ===${NC}"
echo ""

# 配置
DEMO_DIR="/tmp/tailr-demo"
LOG_FILE="$DEMO_DIR/app.log"
PORT=7700

# 创建演示目录
mkdir -p "$DEMO_DIR"

echo -e "${YELLOW}1. 生成示例日志文件...${NC}"

# 生成不同级别的日志
cat > "$LOG_FILE" << 'EOF'
2026-06-23T10:00:01Z [INFO] Application started successfully
2026-06-23T10:00:02Z [INFO] Loading configuration from /etc/app/config.toml
2026-06-23T10:00:03Z [DEBUG] Initializing database connection pool
2026-06-23T10:00:04Z [INFO] Database connection established (pool size: 10)
2026-06-23T10:00:05Z [INFO] Starting HTTP server on 0.0.0.0:8080
2026-06-23T10:00:06Z [INFO] Server listening on port 8080
2026-06-23T10:00:10Z [INFO] GET /api/users - 200 OK (12ms)
2026-06-23T10:00:11Z [INFO] GET /api/products - 200 OK (8ms)
2026-06-23T10:00:12Z [WARN] Slow query detected: SELECT * FROM orders (450ms)
2026-06-23T10:00:13Z [INFO] POST /api/orders - 201 Created (25ms)
2026-06-23T10:00:14Z [ERROR] Failed to connect to payment gateway: Connection timeout
2026-06-23T10:00:15Z [INFO] GET /api/users/123 - 200 OK (5ms)
2026-06-23T10:00:16Z [DEBUG] Cache hit for key: user:123
2026-06-23T10:00:17Z [WARN] Memory usage at 85% threshold
2026-06-23T10:00:18Z [INFO] GET /api/products/456 - 200 OK (3ms)
2026-06-23T10:00:19Z [ERROR] Validation failed: email field is required
2026-06-23T10:00:20Z [INFO] POST /api/users - 201 Created (18ms)
2026-06-23T10:00:21Z [DEBUG] Sending welcome email to user@example.com
2026-06-23T10:00:22Z [INFO] Email sent successfully
2026-06-23T10:00:23Z [WARN] Rate limit approaching for IP 192.168.1.100
2026-06-23T10:00:24Z [INFO] GET /api/health - 200 OK (1ms)
2026-06-23T10:00:25Z [ERROR] Unhandled exception in request handler
2026-06-23T10:00:25Z [ERROR] Stack trace: NullPointerException at UserService.java:42
2026-06-23T10:00:26Z [INFO] Request completed with error (request_id: abc123)
2026-06-23T10:00:27Z [DEBUG] Garbage collection completed (freed: 128MB)
2026-06-23T10:00:28Z [INFO] GET /api/dashboard - 200 OK (150ms)
2026-06-23T10:00:29Z [WARN] Disk usage at 90% threshold
2026-06-23T10:00:30Z [INFO] Scheduled task started: cleanup_old_sessions
2026-06-23T10:00:31Z [INFO] Cleaned up 1250 expired sessions
2026-06-23T10:00:32Z [DEBUG] Session cleanup completed in 2.3s
2026-06-23T10:00:33Z [INFO] GET /api/reports/daily - 200 OK (320ms)
2026-06-23T10:00:34Z [ERROR] Database connection lost, attempting reconnect...
2026-06-23T10:00:35Z [INFO] Database connection restored
2026-06-23T10:00:36Z [INFO] POST /api/webhooks - 200 OK (45ms)
2026-06-23T10:00:37Z [DEBUG] Webhook payload processed: order_created
2026-06-23T10:00:38Z [WARN] High CPU usage detected: 92%
2026-06-23T10:00:39Z [INFO] GET /api/users/search?q=john - 200 OK (22ms)
2026-06-23T10:00:40Z [INFO] Search returned 15 results
2026-06-23T10:00:41Z [ERROR] External API request failed: 503 Service Unavailable
2026-06-23T10:00:42Z [INFO] Retrying request (attempt 1/3)
2026-06-23T10:00:43Z [INFO] External API request succeeded on retry
2026-06-23T10:00:44Z [DEBUG] Response cached for 5 minutes
2026-06-23T10:00:45Z [INFO] GET /api/products?category=electronics - 200 OK (18ms)
2026-06-23T10:00:46Z [INFO] Response sent: 42 products
2026-06-23T10:00:47Z [WARN] SSL certificate expires in 7 days
2026-06-23T10:00:48Z [INFO] Health check passed
2026-06-23T10:00:49Z [DEBUG] Memory stats: used=2.1GB, free=1.9GB, total=4.0GB
2026-06-23T10:00:50Z [INFO] Application running normally
EOF

echo -e "${GREEN}   ✓ 生成了 $(wc -l < "$LOG_FILE") 行示例日志${NC}"
echo ""

echo -e "${YELLOW}2. 启动 tailr 服务器...${NC}"
echo -e "   运行命令: ${GREEN}tailr --log $DEMO_DIR -b 127.0.0.1:$PORT${NC}"
echo ""

echo -e "${YELLOW}3. 录制 GIF 的步骤:${NC}"
echo ""
echo "   使用以下工具之一录制 GIF:"
echo "   - macOS: Kap (https://getkap.co/) 或 Giphy Capture"
echo "   - Linux: Peek 或 Byzanz"
echo "   - Windows: ScreenToGif"
echo ""
echo "   录制场景 (建议 10-15 秒):"
echo ""
echo "   场景 1: 实时日志流 (3 秒)"
echo "   - 打开浏览器访问 http://127.0.0.1:$PORT"
echo "   - 展示实时日志流更新"
echo ""
echo "   场景 2: 日志级别过滤 (3 秒)"
echo "   - 点击顶部的 ERROR 标签"
echo "   - 展示只显示错误日志"
echo ""
echo "   场景 3: 关键词搜索 (3 秒)"
echo "   - 在搜索框输入 'database'"
echo "   - 展示搜索结果高亮"
echo ""
echo "   场景 4: 设置页面 (2 秒)"
echo "   - 打开设置对话框"
echo "   - 展示日志级别配置"
echo ""
echo "   场景 5: 实时尾随 (2 秒)"
echo "   - 点击尾随按钮"
echo "   - 展示自动滚动到最新日志"
echo ""

echo -e "${YELLOW}4. 后台持续写入日志 (可选):${NC}"
echo ""
echo "   在另一个终端运行:"
echo "   ${GREEN}while true; do echo \"\$(date -u +%Y-%m-%dT%H:%M:%SZ) [INFO] New log entry \$RANDOM\" >> $DEMO_DIR/app.log; sleep 1; done${NC}"
echo ""

echo -e "${YELLOW}5. 优化 GIF 文件大小:${NC}"
echo ""
echo "   - 使用 https://ezgif.com/ 优化 GIF"
echo "   - 建议尺寸: 800x450 或 1280x720"
echo "   - 帧率: 10-15 fps"
echo "   - 最终大小: < 5MB"
echo ""

echo -e "${GREEN}=== 准备完成 ===${NC}"
echo ""
echo "将录制的 GIF 保存为 docs/demo.gif，然后更新 README.md:"
echo "  取消注释: ![tailr demo](docs/demo.gif)"
echo ""
