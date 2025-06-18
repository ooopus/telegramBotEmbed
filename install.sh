#!/bin/bash

# ==============================================================================
# Telegram QA Bot (telembed) 自动安装与部署脚本
#
# 该脚本会自动下载、配置并以 systemd 服务的形式运行机器人。
# 运行方式:
# curl -sSL https://raw.githubusercontent.com/ooopus/telegramBotEmbed/refs/heads/main/install.sh | sudo bash
# ==============================================================================

set -e # 如果任何命令失败，则立即退出

# --- 配置变量 ---
SERVICE_NAME="telembed"
SERVICE_USER="telembed"
SERVICE_GROUP="telembed"

# 工作目录 (用于存放数据和缓存)
WORK_DIR="/opt/telembed"
# 配置目录 (根据 service 文件中的 XDG_CONFIG_HOME=/opt 和代码逻辑)
CONFIG_DIR_BASE="/opt/telegramBotEmbed"

# 日志目录
LOG_DIR="/var/log/${SERVICE_NAME}"

# 二进制文件下载链接
BINARY_URL="https://github.com/ooopus/telegramBotEmbed/releases/latest/download/telembed-x86_64-unknown-linux-gnu"
# systemd 服务文件下载链接
SERVICE_URL="https://raw.githubusercontent.com/ooopus/telegramBotEmbed/refs/heads/main/telembed.service"


# --- 脚本开始 ---
echo "🚀 开始部署 Telegram QA Bot (${SERVICE_NAME})..."

# 检查是否以 root 权限运行
if [ "$(id -u)" -ne 0 ]; then
    echo "❌ 请以 root 权限运行此脚本 (例如: sudo ./install.sh)" >&2
    exit 1
fi

# 1. 停止可能正在运行的旧服务
echo "🔄 正在停止旧的服务 (如果存在)..."
systemctl stop "${SERVICE_NAME}.service" > /dev/null 2>&1 || true

# 2. 创建用户和组 (如果不存在)
echo "👤 正在创建系统用户和组 '${SERVICE_USER}'..."
if ! getent group "${SERVICE_GROUP}" > /dev/null; then
    groupadd --system "${SERVICE_GROUP}"
    echo "  - 用户组 '${SERVICE_GROUP}' 已创建。"
else
    echo "  - 用户组 '${SERVICE_GROUP}' 已存在。"
fi

if ! id "${SERVICE_USER}" > /dev/null 2>&1; then
    useradd --system --no-create-home --shell /bin/false -g "${SERVICE_GROUP}" "${SERVICE_USER}"
    echo "  - 系统用户 '${SERVICE_USER}' 已创建。"
else
    echo "  - 系统用户 '${SERVICE_USER}' 已存在。"
fi

# 3. 创建目录结构
echo "📁 正在创建目录..."
mkdir -p "${WORK_DIR}/data"
mkdir -p "${WORK_DIR}/cache"
mkdir -p "${CONFIG_DIR_BASE}"
mkdir -p "${LOG_DIR}"
echo "  - 工作目录: ${WORK_DIR}"
echo "  - 配置目录: ${CONFIG_DIR_BASE}"
echo "  - 日志目录: ${LOG_DIR}"

# 4. 下载并安装二进制文件
echo "🔽 正在从 GitHub 下载二进制文件..."
curl -L "${BINARY_URL}" -o "/usr/local/bin/${SERVICE_NAME}"
chmod +x "/usr/local/bin/${SERVICE_NAME}"
echo "  - 二进制文件已安装到 /usr/local/bin/${SERVICE_NAME}"

# 5. 下载并安装 systemd 服务文件
echo "⚙️  正在安装 systemd 服务文件..."
curl -L "${SERVICE_URL}" -o "/etc/systemd/system/${SERVICE_NAME}.service"
echo "  - 服务文件已安装到 /etc/systemd/system/${SERVICE_NAME}.service"

QA_FILE_PATH="${WORK_DIR}/data/QA.json"
if [ ! -f "${QA_FILE_PATH}" ]; then
    echo "📚 正在创建空的知识库文件 ${QA_FILE_PATH}..."
    echo "[]" > "${QA_FILE_PATH}"
else
    echo "  - 找到已存在的知识库文件，将不会覆盖。"
fi

# 6. 设置权限
echo "🔐 正在设置目录和文件权限..."
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${WORK_DIR}"
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${CONFIG_DIR_BASE}"
chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${LOG_DIR}"
chmod -R 750 "${WORK_DIR}"
chmod -R 750 "${CONFIG_DIR_BASE}"
chmod -R 750 "${LOG_DIR}"
echo "  - 权限设置完成。"

# 7. 重载并启动服务
echo "🚀 正在启动服务..."
systemctl daemon-reload
systemctl enable "${SERVICE_NAME}.service"
systemctl start "${SERVICE_NAME}.service"

echo ""
echo "✅ 部署完成!"
echo ""
echo "--- 🚨 重要提示 🚨 ---"
echo "机器人服务已启动，但您必须编辑配置文件才能让它正常工作："
echo "👉 sudo nano ${CONFIG_FILE_PATH}"
echo ""
echo "请在文件中填入您的 [telegram] token, super_admins 和 [embedding] api_keys。"
echo "修改并保存后，请使用以下命令重启服务："
echo "👉 sudo systemctl restart ${SERVICE_NAME}"
echo "--------------------------"
echo ""
echo "您可以使用以下命令检查服务状态："
echo "  sudo systemctl status ${SERVICE_NAME}"
echo ""
echo "查看实时日志："
echo "  sudo journalctl -u ${SERVICE_NAME} -f"
