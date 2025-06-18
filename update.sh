#!/bin/bash

# ==============================================================================
# Telegram QA Bot (telembed) 自动更新脚本
#
# 该脚本会自动下载最新版本的机器人程序和服务文件，并重启服务。
# 它不会触碰您的数据和配置文件。
# 运行方式:
# curl -sSL https://raw.githubusercontent.com/ooopus/telembed/refs/heads/main/update.sh | sudo bash
# 或者直接: sudo ./update.sh
# ==============================================================================

set -e # 如果任何命令失败，则立即退出

# --- 配置变量 (与 install.sh 保持一致) ---
SERVICE_NAME="telembed"
SERVICE_USER="telembed"
SERVICE_GROUP="telembed"
WORK_DIR="/opt/telembed"
CONFIG_DIR_BASE="/opt/telembed"
LOG_DIR="/var/log/${SERVICE_NAME}"

# 二进制文件下载链接
BINARY_URL="https://github.com/ooopus/telembed/releases/latest/download/telembed-x86_64-unknown-linux-gnu"
# systemd 服务文件下载链接
SERVICE_URL="https://raw.githubusercontent.com/ooopus/telembed/refs/heads/main/telembed.service"


# --- 脚本开始 ---
echo "🚀 开始更新 Telegram QA Bot (${SERVICE_NAME})..."

# 1. 检查是否以 root 权限运行
if [ "$(id -u)" -ne 0 ]; then
    echo "❌ 请以 root 权限运行此脚本 (例如: sudo ./update.sh)" >&2
    exit 1
fi

echo "✅ Root 权限检查通过。"

# 2. 停止当前正在运行的服务
echo "🔄 正在停止 ${SERVICE_NAME} 服务..."
if systemctl is-active --quiet "${SERVICE_NAME}.service"; then
    systemctl stop "${SERVICE_NAME}.service"
    echo "   - 服务已停止。"
else
    echo "   - 服务未在运行，无需停止。"
fi

# 3. 下载并替换二进制文件
echo "🔽 正在从 GitHub 下载最新的二进制文件..."
# 使用 mktemp 创建一个临时文件来安全地下载
TMP_BINARY=$(mktemp)
curl -L --fail "${BINARY_URL}" -o "${TMP_BINARY}"
echo "   - 下载完成，正在替换旧文件..."
mv "${TMP_BINARY}" "/usr/local/bin/${SERVICE_NAME}"
chmod +x "/usr/local/bin/${SERVICE_NAME}"
echo "   - 二进制文件已更新到 /usr/local/bin/${SERVICE_NAME}"

# 4. 下载并替换 systemd 服务文件
echo "⚙️  正在下载最新的 systemd 服务文件..."
# 这很重要，因为服务依赖项或参数可能会改变
curl -L --fail "${SERVICE_URL}" -o "/etc/systemd/system/${SERVICE_NAME}.service"
echo "   - 服务文件已更新到 /etc/systemd/system/${SERVICE_NAME}.service"

# 5. 确保文件和目录权限仍然正确
# 这可以防止因某些意外操作导致权限错误
echo "🔐 正在验证并设置权限..."
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${WORK_DIR}"
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${CONFIG_DIR_BASE}"
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${LOG_DIR}"
chmod -R 750 "${WORK_DIR}"
chmod -R 750 "${CONFIG_DIR_BASE}"
chmod -R 750 "${LOG_DIR}"
echo "   - 权限验证完成。"

# 6. 重载 systemd 并重启服务
echo "🚀 正在重启服务以应用更新..."
systemctl daemon-reload
systemctl restart "${SERVICE_NAME}.service"

echo ""
echo "✅ 更新完成!"
echo ""
echo "机器人服务已使用最新版本重新启动。"
echo "您的配置文件和知识库数据 (${WORK_DIR}/data/QA.json) 都已保留。"
echo ""
echo "您可以使用以下命令检查服务状态："
echo "   sudo systemctl status ${SERVICE_NAME}"
echo ""
echo "查看实时日志："
echo "   sudo journalctl -u ${SERVICE_NAME} -f"
echo ""
echo "💡 建议: 访问项目的 GitHub Releases 页面，查看更新日志以了解新功能或重大变更。"