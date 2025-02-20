import tomli
import os

# 导入配置
try:
    with open('config.toml', 'rb') as f:
        config = tomli.load(f)
        print("成功加载 config.toml 配置文件")
except Exception as e:
    print("未找到 config.toml 文件或读取失败")
    print("请参考 config.example.toml 创建配置文件")
    print(f"错误信息: {e}")
    raise SystemExit(1)

# 从配置中获取值
TOKEN = config['token']
API_KEY = config['api_key']
EMBED_API_URL = config['embed_api_url']
EMBED_MODEL = config['embed_model']
CACHE_DIR = config['cache_dir']
SIMILARITY_THRESHOLD = config['similarity_threshold']
DELETE_DELAY = config['delete_delay']
MESSAGE_TIMEOUT = config['message_timeout']

# 确保缓存目录存在
if not os.path.exists(CACHE_DIR):
    os.makedirs(CACHE_DIR)