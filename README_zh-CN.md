# telegramBotEmbed

这是一个基于 Telegram 的问答机器人，它使用语义相似度来匹配用户的问题并从预定义的问答数据中返回答案。

## 功能

-   **自动问答匹配：** 在群组和私聊中，机器人会自动寻找与用户输入消息最匹配的问题，并返回相应的答案。
-   **语义相似度搜索：** 使用 BAAI/bge-m3 模型计算文本嵌入向量，并通过余弦相似度找到最佳匹配。
-   **缓存机制：** 为了提高性能和减少 API 调用，机器人会缓存计算过的嵌入向量。
-   **可配置的相似度阈值：** 可以调整相似度阈值来控制匹配的严格程度。
-   **基本命令：** 支持 `/start` 和 `/help` 命令。

## 技术细节

-   **语言模型：** 使用 BAAI/bge-m3 模型生成文本嵌入。
-   **嵌入 API:** 通过 SiliconFlow API 获取文本嵌入。
-   **相似度计算：** 使用余弦相似度来比较查询文本和预定义问题的嵌入向量。
-   **缓存：** 将嵌入向量和 QA 数据的哈希值保存在本地文件中，以加快后续查询速度。
-   **Telegram API:** 使用 [pyTelegramBotAPI](https://github.com/eternnoir/pyTelegramBotAPI) 与 Telegram API 交互。

## 文件结构

-   `/opt/telegramBotEmbed/main.py`: 机器人主程序。
-   `/opt/telegramBotEmbed/docs/QA.json`: 问答数据文件。
-   `/opt/telegramBotEmbed/cache/`: 缓存文件夹, 包含以下两个文件
    -   `embeddings_cache_{model_name}.npz`: 存储计算好的文本嵌入向量
    -   `qa_hash_{model_name}.txt`: 存储QA数据的哈希值

## 安装和配置

### 1. 系统要求

-   Linux 系统 (推荐使用 systemd 管理服务的发行版，如 Ubuntu、Debian、CentOS 等)。
-   **Python 3.6 或更高版本 (推荐 Python 3.8+)。**  **如果你的系统没有安装 Python 3 或版本过低，请先安装 Python。**
-   **强烈推荐使用 Python 虚拟环境 (.venv)**。

#### 安装 Python 3

**在大多数 Linux 发行版上，Python 3 已经预装。 你可以先检查 Python 版本：**

```bash
python3 --version
# 或
python --version # 如果 python 默认指向 python3
```

**如果 Python 3 未安装或版本过低，请根据你的 Linux 发行版选择合适的命令安装：**

*   **Debian/Ubuntu 或类似发行版:**

    ```bash
    sudo apt update  # 更新软件包列表 (推荐)
    sudo apt install python3 python3-pip python3-venv
    ```

*   **CentOS/RHEL 或类似发行版:**

    ```bash
    sudo yum update  # 更新软件包列表 (推荐)
    sudo yum install python3 python3-pip python3-venv
    # 或 (如果 yum 不可用，尝试 dnf)
    # sudo dnf update
    # sudo dnf install python3 python3-pip python3-venv
    ```

    *   **注意:**  在某些较旧的 CentOS/RHEL 系统上，你可能需要启用 EPEL (Extra Packages for Enterprise Linux) 仓库才能安装较新版本的 Python 和 pip。

*   **其他发行版:**  请根据你的发行版文档查找安装 Python 3、pip 和 venv 的方法。 通常包名类似 `python3`, `python3-pip`, `python3-venv` 或 `python-pip`, `python-virtualenv` 等。

**安装完成后，再次检查 Python 3 版本以确认安装成功。**

### 2. 创建用户 (强烈建议)

出于安全考虑，建议创建一个非 root 用户来运行机器人。

```bash
sudo adduser --no-create-home -s /usr/sbin/nologin tgbot  # 创建一个名为 tgbot 的系统用户
```

### 3. 安装机器人

```bash
sudo git clone https://github.com/ooopus/telegramBotEmbed /opt/telegramBotEmbed
```

### 4. 创建和激活 Python 虚拟环境 (.venv)

**强烈推荐使用虚拟环境来隔离项目依赖，避免与系统 Python 环境冲突。**

```bash
sudo　python3 -m venv /opt/telegramBotEmbed/.venv # 创建虚拟环境 (可能需要安装 python3-venv 包: sudo apt install python3-venv)

# 激活虚拟环境 (每次启动机器人前都需要激活)
sudo source /opt/telegramBotEmbed/.venv/bin/activate

# 如果你需要保持在当前用户下操作, 可以先切换到 tgbot 用户再激活:
# sudo su - tgbot
# source /opt/telegramBotEmbed/.venv/bin/activate
# (.venv) tgbot@yourserver:~$  # 提示符前出现 (.venv) 表示虚拟环境已激活
```
**注意:**  `/usr/bin/python3` 是系统 Python 解释器路径，请根据你的系统实际情况调整。 可以使用 `which python3` 命令查找。

### 5. 安装依赖 (在虚拟环境中)

**确保虚拟环境已激活 (提示符前有 `(.venv)`)** 再执行以下命令。

```bash
cd /opt/telegramBotEmbed
sudo /opt/telegramBotEmbed/.venv/bin/pip3 install -r requirements.txt
# 或者，如果已切换到 tgbot 用户并激活了 .venv:
# pip3 install pyTelegramBotAPI requests numpy
# 或 (更常用):
# pip install pyTelegramBotAPI requests numpy
```
**注意:**  `/opt/telegramBotEmbed/.venv/bin/pip3` 是虚拟环境中的 `pip`， 确保使用虚拟环境中的 `pip` 安装依赖。

### 6. 准备问答数据

-   创建 `docs` 目录：
    ```bash
    sudo mkdir /opt/telegramBotEmbed/docs
    ```

-   创建 `QA.json` 文件：
    ```bash
    sudo nano /opt/telegramBotEmbed/docs/QA.json
    ```
-   `QA.json` 文件应包含一个 JSON 数组，每个元素是一个包含 "question" 和 "answer" 键值对的对象。例如：

    ```json
    [
        {
            "question": "什么是最好的编程语言？",
            "answer": "没有绝对最好的编程语言，取决于具体应用场景。"
        },
        {
            "question": "如何学习 Python？",
            "answer": "可以通过在线教程、书籍、项目实践等方式学习 Python。"
        }
    ]
    ```

### 7. 创建 `cache` 目录

```bash
sudo mkdir /opt/telegramBotEmbed/cache
```

### 8. 配置 config.toml

```bash
sudo mv /opt/telegramBotEmbed/config.example.toml /opt/telegramBotEmbed/config.toml
```

在 `/opt/telegramBotEmbed/config.toml` 文件中配置以下内容：

-   **Telegram Bot Token:**  将 `TOKEN` 变量设置为你的 Telegram 机器人 Token。
-   **API 密钥:** 将 `API_KEY` 变量设置为你的 SiliconFlow API 密钥。
-   **相似度阈值：** 调整 `SIMILARITY_THRESHOLD` 变量（默认为 0.7）来控制匹配的严格程度。
-   **DELETE_DELAY:** 调整 `DELETE_DELAY` 变量（默认为 10）来控制消息自动删除的延迟时间。

### 9. 更改文件所有权 (重要)

```bash
sudo chown -R tgbot:tgbot /opt/telegramBotEmbed
```



## 使用 Systemd 管理机器人 (推荐)

### 1. 创建 Systemd 配置文件

```bash
sudo nano /etc/systemd/system/telegram-qa-bot.service
```

将以下内容复制到文件中：

```systemd
[Unit]
Description=Telegram QA Bot
After=network.target
StartLimitIntervalSec=0

[Service]
Type=simple
Restart=always
RestartSec=1
User=tgbot
WorkingDirectory=/opt/telegramBotEmbed
ExecStart=/opt/telegramBotEmbed/.venv/bin/python3 /opt/telegramBotEmbed/main.py  # 使用虚拟环境中的 Python
Environment="PYTHONPATH=/opt/telegramBotEmbed"

[Install]
WantedBy=multi-user.target
```
**注意:** `ExecStart` 行指定了使用虚拟环境 `/opt/telegramBotEmbed/.venv/bin/python3` 中的 Python 解释器。

### 2. 管理服务

-   重新加载 systemd 配置：
    ```bash
    sudo systemctl daemon-reload
    ```

-   启用服务（开机自启）：
    ```bash
    sudo systemctl enable telegram-qa-bot.service
    ```

-   启动服务：
    ```bash
    sudo systemctl start telegram-qa-bot.service
    ```

-   查看服务状态：
    ```bash
    sudo systemctl status telegram-qa-bot.service
    ```

-   停止服务：
    ```bash
    sudo systemctl stop telegram-qa-bot.service
    ```
-   查看日志:
    ```bash
    sudo journalctl -u telegram-qa-bot.service -f
    ```

## 与机器人交互

-   在 Telegram 中与机器人进行私聊。
-   将机器人添加到群组中，并给予管理员（否则无法接收群组消息）。
-   使用 `/start` 命令开始对话。
-   使用 `/help` 命令查看帮助信息。

## 更新和维护

```bash
cd /opt/telegramBotEmbed
sudo -u tgbot git fetch origin
sudo -u tgbot git pull origin main
sudo chown -R tgbot:tgbot /opt/telegramBotEmbed
sudo -u tgbot /opt/telegramBotEmbed/.venv/bin/pip3 install -r requirements.txt
sudo systemctl stop telegram-qa-bot.service
sudo systemctl start telegram-qa-bot.service
```
