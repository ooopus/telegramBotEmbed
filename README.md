# Telembed

Telembed is a sophisticated Telegram bot designed to answer questions within group chats based on a curated knowledge base. It leverages the power of Google's Gemini embedding models to perform semantic searches, ensuring that it can find the most relevant answer even if the user's query doesn't use the exact keywords.

The bot provides a seamless experience for both users asking questions and administrators managing the knowledge base, with all Q&A management handled directly through an interactive Telegram interface.

## Features

-   **Semantic Q&A:** Goes beyond simple keyword matching. Understands the *meaning* of a user's question to find the best answer from its knowledge base.
-   **Interactive Q&A Management:** Administrators can add, list, search, edit, and delete question-and-answer pairs directly within Telegram using intuitive commands and inline keyboards.
-   **Robust API Key Management:** Intelligently manages multiple Gemini API keys, supporting round-robin usage, rate-limiting (RPM/RPD), and automatic temporary disabling of keys that exceed their quota.
-   **Persistent Storage:** The entire knowledge base of Q&A pairs is stored in a simple `JSON` file.
-   **Efficient Embedding Cache:** Automatically caches question embeddings to minimize API calls to Gemini, reducing costs and speeding up responses.
-   **Configurable and Secure:**
    -   Restrict bot usage to specific group chats.
    -   Define "super admins" who have universal privileges.
    -   Relies on group administrator permissions for management actions.
-   **Clean Chat Interface:** Automatically deletes user commands and bot responses after a configurable delay to keep group chats tidy.

## How It Works

The bot's intelligence is based on a concept called "embeddings," which are numerical representations of text.

1.  **Initialization:** On startup, the bot loads all Q&A pairs from its `QA.json` file.
2.  **Embedding:** For each question, it calls the Gemini API to generate a vector embedding. These embeddings are stored in an on-disk cache to prevent redundant API calls on subsequent restarts.
3.  **User Query:** When a user sends a message in a configured group, the bot treats it as a potential question.
4.  **Semantic Search:** The bot generates an embedding for the user's query and uses **cosine similarity** to compare it against the embeddings of all questions in its knowledge base.
5.  **Matching & Response:** If the highest similarity score is above a configured threshold (e.g., `0.85`), it means a confident match has been found. The bot then posts the corresponding answer. If no match meets the threshold, the bot remains silent to avoid spamming incorrect answers.
6.  **Knowledge Base Management:** When an admin adds, edits, or deletes a Q&A pair, the bot updates the `QA.json` file and automatically reloads and re-generates its in-memory embeddings to reflect the changes instantly.

## Installation and Configuration

1.  **Prerequisites:** You need to have the Rust toolchain installed.
2.  **Clone the Repository:**
    ```bash
    git clone https://github.com/ooopus/telembed.git
    cd telembed
    ```
3.  **Configuration:**
    The first time you run the bot, it will automatically create a `config.toml` file in your system's configuration directory (e.g., `~/.config/telembed/config.toml` on Linux). You need to edit this file with your details.

    ```toml
    # src/config/types.rs
    [telegram]
    token = "YOUR_TELEGRAM_BOT_TOKEN" # Get this from BotFather
    super_admins = [123456789]        # Your numeric Telegram User ID
    allowed_group_ids = [-1001234567890] # IDs of groups where the bot should operate

    [embedding]
    api_keys = ["YOUR_GEMINI_API_KEY_1", "YOUR_GEMINI_API_KEY_2"] # Get from Google AI Studio
    model = "gemini-embedding-exp-03-07"
    ndims = 3072
    rpm = 5  # Requests-Per-Minute limit per key
    rpd = 100 # Requests-Per-Day limit per key

    [similarity]
    threshold = 0.85 # Confidence threshold for a match (0.0 to 1.0)

    [message]
    delete_delay = 10 # Seconds before deleting bot's answer
    timeout = 60      # Ignore messages older than 60 seconds

    [qa]
    qa_json_path = "data/QA.json" # Path to your knowledge base file
    ```

4.  **Create the Knowledge Base:**
    Create the `data` directory and an empty `QA.json` file.
    ```bash
    mkdir data
    echo "[]" > data/QA.json
    ```

5.  **Run the Bot:**
    ```bash
    cargo run --release
    ```

## Usage (Bot Commands)

All management commands require the user to be an administrator in the group.

-   `/addqa`
    Reply to a user's message with this command to use their message as the **question**. The bot will then prompt you to reply with the corresponding **answer**. Finally, it will ask for confirmation before saving the new Q&A pair.

-   `/answer`
    Reply to a message with this command to force the bot to treat that message as a question and search for an answer.

-   `/listqa`
    Lists all Q&A pairs in an interactive panel. You can click on any question to view, edit, or delete it.

-   `/searchqa <keywords>`
    Searches the questions in the knowledge base for the given keywords and displays the results in an interactive management panel.

-   `/start`
    Displays a simple welcome message.
