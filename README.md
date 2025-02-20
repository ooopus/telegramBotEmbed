# telegramBotEmbed

This is a Telegram-based Q&A bot that uses semantic similarity to match user questions and return answers from predefined Q&A data.

## Features

- **Automatic Q&A Matching:** In groups and private chats, the bot automatically finds the best match for user input and returns the corresponding answer.
- **Semantic Similarity Search:** Uses the BAAI/bge-m3 model to compute text embeddings and find the best match via cosine similarity.
- **Caching Mechanism:** To improve performance and reduce API calls, the bot caches computed embeddings.
- **Configurable Similarity Threshold:** Adjust the similarity threshold to control the strictness of matching.
- **Basic Commands:** Supports `/start` and `/help` commands.

## Technical Details

- **Language Model:** Uses the BAAI/bge-m3 model to generate text embeddings.
- **Embedding API:** Retrieves text embeddings via the SiliconFlow API.
- **Similarity Calculation:** Uses cosine similarity to compare query text with predefined question embeddings.
- **Caching:** Saves embeddings and Q&A data hashes locally to speed up subsequent queries.
- **Telegram API:** Interacts with the Telegram API using [pyTelegramBotAPI](https://github.com/eternnoir/pyTelegramBotAPI).

## File Structure

- `/opt/telegramBotEmbed/main.py`: The main bot program.
- `/opt/telegramBotEmbed/docs/QA.json`: Q&A data file.
- `/opt/telegramBotEmbed/cache/`: Cache folder, containing the following files:
  - `embeddings_cache_{model_name}.npz`: Stores computed text embeddings.
  - `qa_hash_{model_name}.txt`: Stores the hash of Q&A data.

## Installation and Configuration

### 1. System Requirements

- Linux system (recommended distributions with systemd, such as Ubuntu, Debian, CentOS, etc.).
- **Python 3.6 or higher (recommended Python 3.8+).** If your system does not have Python 3 installed or the version is too low, please install Python first.
- **Strongly recommended to use a Python virtual environment (.venv).**

#### Installing Python 3

**On most Linux distributions, Python 3 is pre-installed. You can check the Python version first:**

```bash
python3 --version
# or
python --version # if python points to python3
```

**If Python 3 is not installed or the version is too low, choose the appropriate command for your Linux distribution:**

* **Debian/Ubuntu or similar distributions:**

    ```bash
    sudo apt update  # Update package list (recommended)
    sudo apt install python3 python3-pip python3-venv
    ```

* **CentOS/RHEL or similar distributions:**

    ```bash
    sudo yum update  # Update package list (recommended)
    sudo yum install python3 python3-pip python3-venv
    # or (if yum is not available, try dnf)
    # sudo dnf update
    # sudo dnf install python3 python3-pip python3-venv
    ```

    * **Note:** On some older CentOS/RHEL systems, you may need to enable the EPEL (Extra Packages for Enterprise Linux) repository to install newer versions of Python and pip.

* **Other distributions:** Refer to your distribution's documentation to find the method for installing Python 3, pip, and venv. Typically, the package names are similar to `python3`, `python3-pip`, `python3-venv`, or `python-pip`, `python-virtualenv`, etc.

**After installation, check the Python 3 version again to confirm successful installation.**

### 2. Create a User (Strongly Recommended)

For security reasons, it is recommended to create a non-root user to run the bot.

```bash
sudo adduser --no-create-home -s /usr/sbin/nologin tgbot  # Create a system user named tgbot
```

### 3. Install the Bot

```bash
sudo git clone https://github.com/ooopus/telegramBotEmbed /opt/telegramBotEmbed

```

### 4. Create and Activate Python Virtual Environment (.venv)

**Strongly recommended to use a virtual environment to isolate project dependencies and avoid conflicts with the system Python environment.**

```bash
sudo python3 -m venv /opt/telegramBotEmbed/.venv # Create a virtual environment (may need to install python3-venv package: sudo apt install python3-venv)

# Activate the virtual environment (required before starting the bot)
sudo source /opt/telegramBotEmbed/.venv/bin/activate

# If you need to operate under the current user, switch to the tgbot user first:
# sudo su - tgbot
# source /opt/telegramBotEmbed/.venv/bin/activate
# (.venv) tgbot@yourserver:~$  # The (.venv) prefix indicates the virtual environment is activated
```
**Note:** `/usr/bin/python3` is the system Python interpreter path. Adjust according to your system. Use `which python3` to locate the path.

### 5. Install Dependencies (in the Virtual Environment)

**Ensure the virtual environment is activated (with `(.venv)` prefix)** before running the following commands.

```bash
cd /opt/telegramBotEmbed
sudo /opt/telegramBotEmbed/.venv/bin/pip3 install -r requirements.txt
# Or, if already switched to the tgbot user and activated .venv:
# pip3 install pyTelegramBotAPI requests numpy
# Or (more commonly):
# pip install pyTelegramBotAPI requests numpy
```
**Note:** `/opt/telegramBotEmbed/.venv/bin/pip3` is the `pip` in the virtual environment. Ensure you use the virtual environment's `pip` to install dependencies.

### 6. Prepare Q&A Data

- Create the `docs` directory:
    ```bash
    sudo mkdir /opt/telegramBotEmbed/docs
    ```

- Create the `QA.json` file:
    ```bash
    sudo nano /opt/telegramBotEmbed/docs/QA.json
    ```
- The `QA.json` file should contain a JSON array, where each element is an object with "question" and "answer" key-value pairs. For example:

    ```json
    [
        {
            "question": "What is the best programming language?",
            "answer": "There is no absolute best programming language; it depends on the specific use case."
        },
        {
            "question": "How to learn Python?",
            "answer": "You can learn Python through online tutorials, books, and project practice."
        }
    ]
    ```

### 7. Create the `cache` Directory

```bash
sudo mkdir /opt/telegramBotEmbed/cache
```

### 8. Configure config.py

```bash
sudo mv /opt/telegramBotEmbed/config.example.toml /opt/telegramBotEmbed/config.toml
```

In the `/opt/telegramBotEmbed/config.toml` file, configure the following:

- **Telegram Bot Token:** Set the `TOKEN` variable to your Telegram bot token.
- **API Key:** Set the `API_KEY` variable to your SiliconFlow API key.
- **Similarity Threshold:** Adjust the `SIMILARITY_THRESHOLD` variable (default is 0.7) to control the strictness of matching.
- **DELETE_DELAY:** Adjust the `DELETE_DELAY` variable (default is 10) to control the delay for automatic message deletion.

### 9. Change File Ownership (Important)

```bash
sudo chown -R tgbot:tgbot /opt/telegramBotEmbed
```

## Managing the Bot with Systemd (Recommended)

### 1. Create a Systemd Configuration File

```bash
sudo nano /etc/systemd/system/telegram-qa-bot.service
```

Copy the following content into the file:

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
ExecStart=/opt/telegramBotEmbed/.venv/bin/python3 /opt/telegramBotEmbed/main.py  # Use Python from the virtual environment
Environment="PYTHONPATH=/opt/telegramBotEmbed"

[Install]
WantedBy=multi-user.target
```
**Note:** The `ExecStart` line specifies the use of the Python interpreter in the virtual environment `/opt/telegramBotEmbed/.venv/bin/python3`.

### 2. Manage the Service

- Reload systemd configuration:
    ```bash
    sudo systemctl daemon-reload
    ```

- Enable the service (start on boot):
    ```bash
    sudo systemctl enable telegram-qa-bot.service
    ```

- Start the service:
    ```bash
    sudo systemctl start telegram-qa-bot.service
    ```

- Check service status:
    ```bash
    sudo systemctl status telegram-qa-bot.service
    ```

- Stop the service:
    ```bash
    sudo systemctl stop telegram-qa-bot.service
    ```
- View logs:
    ```bash
    sudo journalctl -u telegram-qa-bot.service -f
    ```

## Interacting with the Bot

- Chat with the bot privately in Telegram.
- Add the bot to a group and grant it admin privileges (otherwise, it cannot receive group messages).
- Use the `/start` command to begin the conversation.
- Use the `/help` command to view help information.

## Updating and Maintenance

```bash
cd /opt/telegramBotEmbed
sudo -u tgbot git fetch origin
sudo -u tgbot git pull origin main
sudo chown -R tgbot:tgbot /opt/telegramBotEmbed
sudo -u tgbot /opt/telegramBotEmbed/.venv/bin/pip3 install -r requirements.txt
sudo systemctl stop telegram-qa-bot.service
sudo systemctl start telegram-qa-bot.service
```