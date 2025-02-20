import telebot
import logging
import requests
import os
import json
import numpy as np
import time
import threading
from config import TOKEN, API_KEY, EMBED_API_URL, EMBED_MODEL, CACHE_DIR, SIMILARITY_THRESHOLD, DELETE_DELAY, MESSAGE_TIMEOUT

# 配置日志
logging.basicConfig(format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
                   level=logging.INFO)
logger = logging.getLogger(__name__)

bot = telebot.TeleBot(TOKEN, skip_pending=True)

# 计算缓存文件路径
model_name = EMBED_MODEL.replace('/', '_').lower()
EMBEDDINGS_CACHE_FILE = os.path.join(CACHE_DIR, f"embeddings_cache_{model_name}.npz")
QA_HASH_FILE = os.path.join(CACHE_DIR, f"qa_hash_{model_name}.txt")

def cosine_similarity(a, b):
    """计算两个向量的余弦相似度"""
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b))

class QAEmbedding:
    def __init__(self):
        self.qa_data = []
        self.question_embeddings = []
        self.load_and_embed_qa()

    def get_embedding(self, text):
        """获取文本的嵌入向量"""
        payload = {
            "model": EMBED_MODEL,
            "input": text
        }
        
        headers = {
            "Authorization": f"Bearer {API_KEY}",
            "Content-Type": "application/json"
        }
        
        try:
            response = requests.post(EMBED_API_URL, json=payload, headers=headers)
            result = response.json()
            
            if not isinstance(result, dict):
                logger.error(f"嵌入API响应格式错误: {result}")
                return None
                
            data = result.get('data', [])
            if not data or not isinstance(data, list):
                logger.error("嵌入API响应中没有data")
                return None
                
            embedding = data[0].get('embedding')
            if not embedding or not isinstance(embedding, list):
                logger.error("嵌入API响应中没有embedding")
                return None
                
            return np.array(embedding)
            
        except Exception as e:
            logger.error(f"获取嵌入向量出错: {e}")
            return None

    def calculate_qa_hash(self, qa_data):
        """计算QA数据的哈希值，用于检测数据是否变化"""
        qa_str = json.dumps(qa_data, sort_keys=True)
        return str(hash(qa_str))

    def load_cached_embeddings(self, qa_hash):
        """从缓存加载嵌入向量"""
        try:
            if not os.path.exists(CACHE_DIR):
                return False
                
            if not os.path.exists(EMBEDDINGS_CACHE_FILE) or not os.path.exists(QA_HASH_FILE):
                return False
                
            # 检查QA数据是否变化
            with open(QA_HASH_FILE, 'r') as f:
                cached_hash = f.read().strip()
                
            if cached_hash != qa_hash:
                return False
                
            # 加载缓存的嵌入向量
            cached_data = np.load(EMBEDDINGS_CACHE_FILE)
            self.question_embeddings = cached_data['embeddings']
            logger.info(f"成功从缓存加载 {len(self.question_embeddings)} 个嵌入向量")
            return True
            
        except Exception as e:
            logger.error(f"加载缓存嵌入向量失败: {e}")
            return False

    def save_embeddings_cache(self, qa_hash):
        """保存嵌入向量到缓存"""
        try:
            if not os.path.exists(CACHE_DIR):
                os.makedirs(CACHE_DIR)
                
            # 保存嵌入向量
            np.savez(EMBEDDINGS_CACHE_FILE, embeddings=self.question_embeddings)
            
            # 保存QA数据哈希值
            with open(QA_HASH_FILE, 'w') as f:
                f.write(qa_hash)
                
            logger.info("成功保存嵌入向量到缓存")
            
        except Exception as e:
            logger.error(f"保存嵌入向量缓存失败: {e}")

    def load_and_embed_qa(self):
        """加载QA数据并计算问题的嵌入向量"""
        try:
            with open('docs/QA.json', 'r', encoding='utf-8') as f:
                self.qa_data = json.load(f)
                logger.info(f"成功加载 {len(self.qa_data)} 条问答数据")
                
            # 计算QA数据的哈希值
            qa_hash = self.calculate_qa_hash(self.qa_data)
            
            # 尝试从缓存加载嵌入向量
            if self.load_cached_embeddings(qa_hash):
                return
                
            # 如果缓存加载失败，重新计算嵌入向量
            self.question_embeddings = []
            embedding_failed = False
            
            for qa in self.qa_data:
                embedding = self.get_embedding(qa["question"])
                if embedding is not None:
                    self.question_embeddings.append(embedding)
                else:
                    logger.error(f"获取问题嵌入向量失败: {qa['question']}")
                    embedding_failed = True
                    break
            
            if embedding_failed:
                logger.error("由于获取嵌入向量失败，跳过缓存保存")
                self.qa_data = []
                self.question_embeddings = []
                return
                    
            self.question_embeddings = np.array(self.question_embeddings)
            logger.info(f"成功计算 {len(self.question_embeddings)} 个问题的嵌入向量")
            
            # 只有在成功获取所有嵌入向量时才保存缓存
            self.save_embeddings_cache(qa_hash)
            
        except Exception as e:
            logger.error(f"加载和嵌入QA数据出错: {e}")
            self.qa_data = []
            self.question_embeddings = []

    def find_matching_qa(self, text):
        """查找与输入文本最匹配的问答对"""
        if not self.qa_data or not len(self.question_embeddings):
            return None
            
        # 获取输入文本的嵌入向量
        query_embedding = self.get_embedding(text)
        if query_embedding is None:
            return None
            
        # 计算与所有问题的余弦相似度
        similarities = [cosine_similarity(query_embedding, q_embedding) for q_embedding in self.question_embeddings]
        
        # 找出最高相似度及其索引
        max_similarity = max(similarities)
        max_index = similarities.index(max_similarity)
        
        logger.info(f"最佳匹配分数: {max_similarity}, 索引: {max_index}")
        
        # 如果相似度超过阈值，返回对应的问答对
        if max_similarity >= SIMILARITY_THRESHOLD:
            matched_qa = self.qa_data[max_index]
            logger.info(f"找到匹配的问题: {matched_qa['question']}, 相似度: {max_similarity}")
            return matched_qa
            
        logger.info(f"没有找到足够相似的问题，最高分数: {max_similarity}")
        return None

# 创建QAEmbedding实例
qa_embedding = QAEmbedding()

def delete_message_later(chat_id, message_id, delay):
    """延迟删除消息的函数"""
    time.sleep(delay)
    try:
        bot.delete_message(chat_id, message_id)
        logger.info(f"已删除消息 - 群组ID：{chat_id}，消息ID：{message_id}")
    except Exception as e:
        logger.error(f"删除消息失败 - 群组ID：{chat_id}，消息ID：{message_id}，错误：{e}")

def escape_html_tags(text: str) -> str:
    """转义HTML标签，但保留blockquote和br标签"""
    # 替换<span>标签
    text = text.replace('<span>', '&lt;span&gt;')
    text = text.replace('</span>', '&lt;/span&gt;')
    # 移除<pre>标签
    text = text.replace('<pre>', '')
    text = text.replace('</pre>', '')
    return text

def format_answer(answer: str) -> str:
    """格式化回答内容为可折叠的引用块"""
    # 先转义HTML标签
    escaped_answer = escape_html_tags(answer)
    return telebot.formatting.hcite(escaped_answer, escape=False, expandable=True)

def is_message_fresh(message) -> bool:
    """检查消息是否在响应时限内"""
    current_time = time.time()
    message_time = message.date
    return (current_time - message_time) <= MESSAGE_TIMEOUT

# 修改群组消息处理函数
@bot.message_handler(func=lambda message: message.chat.type in ['group', 'supergroup'])
def handle_group_message(message):
    if message.text:
        # 检查消息是否在响应时限内
        if not is_message_fresh(message):
            logger.info(f"忽略超时消息 - 群组：{message.chat.title}，用户：{message.from_user.username}")
            return
            
        logger.info(f"收到群组消息 - 群组：{message.chat.title}，用户：{message.from_user.username}，内容：{message.text}")
        
        # 查找匹配的问答
        matched_qa = qa_embedding.find_matching_qa(message.text)
        
        if matched_qa:
            logger.info(f"找到匹配的问题：{matched_qa['question']}")
            # 使用 telebot 的引用块格式化
            formatted_answer = format_answer(matched_qa['answer'])
            # 发送回复并获取发送的消息对象
            sent_message = bot.reply_to(message, formatted_answer, parse_mode="HTML", disable_web_page_preview=True)
            
            # 创建新线程来处理延迟删除
            delete_thread = threading.Thread(
                target=delete_message_later,
                args=(message.chat.id, sent_message.message_id, DELETE_DELAY)
            )
            delete_thread.start()
        else:
            logger.info("没有找到匹配的问题")

# 处理 /start 命令
@bot.message_handler(commands=['start'])
def send_welcome(message):
    bot.reply_to(message, "你好！我是一个问答机器人，可以回答你的问题。")

# 处理 /help 命令
@bot.message_handler(commands=['help'])
def send_help(message):
    help_text = """
可用命令列表：
/start - 开始对话
/help - 显示帮助信息

在群组中，我会自动检测并回答问题。
    """
    bot.reply_to(message, help_text)

# 修改私聊消息处理函数
@bot.message_handler(func=lambda message: message.chat.type == 'private')
def handle_private_message(message):
    if message.text:
        # 检查消息是否在响应时限内
        if not is_message_fresh(message):
            logger.info(f"忽略超时消息 - 用户：{message.from_user.username}")
            return
            
        logger.info(f"收到私聊消息 - 用户：{message.from_user.username}，内容：{message.text}")
        
        # 查找匹配的问答
        matched_qa = qa_embedding.find_matching_qa(message.text)
        
        if matched_qa:
            logger.info(f"找到匹配的问题：{matched_qa['question']}")
            # 使用 telebot 的引用块格式化
            formatted_answer = format_answer(matched_qa['answer'])
            bot.reply_to(message, formatted_answer, parse_mode="HTML", disable_web_page_preview=True)
        else:
            logger.info("没有找到匹配的问题")
            bot.reply_to(message, "抱歉，我没有找到相关的问题和答案。")

def main():
    logger.info("Bot 开始运行...")
    try:
        bot.polling(none_stop=True)
    except Exception as e:
        logger.error(f"Bot 运行出错: {e}")

if __name__ == '__main__':
    main()