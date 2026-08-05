import os
import json
import time
import sys

sys.path.append("/home/bose/projects/camoufox/python")
from camoufox import Camoufox

PROXY_CONFIG = {
    "server": "http://34.118.88.54:3128",
    "username": "relay4855cb91",
    "password": "4gKDPTqhCtFwSvy5FlsDJO91e7A4r3t9"
}

def cleanup_stale_locks(profile_dir: str):
    """Очищает файлы блокировки Firefox перед запуском"""
    for lock_name in [".parentlock", "lock"]:
        lock_path = os.path.join(profile_dir, lock_name)
        if os.path.exists(lock_path) or os.path.islink(lock_path):
            try:
                os.remove(lock_path)
                print(f"[Cleanup] Удален замок: {lock_name}")
            except Exception as e:
                pass

def launch_expert_profile(profile_name: str, target_url: str = "https://e.mail.ru/inbox/"):
    profile_dir = os.path.expanduser(f"~/browser_profiles/{profile_name}")
    
    # 1. Очистка устаревших замков
    cleanup_stale_locks(profile_dir)

    # 2. Проверка и фиксация отпечатка camoufox_config.json
    config_path = os.path.join(profile_dir, "camoufox_config.json")
    config_data = None
    if os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                config_data = json.load(f)
            print(f"[Config] Загружен зафиксированный отпечаток: camoufox_config.json")
        except Exception as e:
            print(f"[Config] Ошибка чтения конфига: {e}")
            config_data = None

    print(f"\n============================================================")
    print(f"Запуск профиля: {profile_name}")
    print(f"Каталог сессии: {profile_dir}")
    print(f"Прямой переход на: {target_url}")
    print(f"============================================================\n")
    
    try:
        camoufox_kwargs = {
            "persistent_context": True,
            "user_data_dir": profile_dir,
            "os": "windows",
            "proxy": PROXY_CONFIG,
            "geoip": True,
            "headless": False
        }
        if config_data:
            camoufox_kwargs["config"] = config_data

        with Camoufox(**camoufox_kwargs) as context:
            page = context.pages[0] if context.pages else context.new_page()

            # Прямой переход без промежуточных сервисов проверки IP
            print(f"Открываем сразу {target_url}...")
            try:
                page.goto(target_url, wait_until="domcontentloaded", timeout=60000)
            except Exception as e:
                print(f"Примечание при загрузке: {e}")
                
            print("\n------------------------------------------------------------")
            print(f"Профиль '{profile_name}' открыт.")
            print("Браузер готов. Для завершения просто закройте окно браузера.")
            print("------------------------------------------------------------\n")
            
            while True:
                time.sleep(5)
                if not context.pages or page.is_closed():
                    print("[ProfileManager] Окно браузера закрыто пользователем. Сессия сохранена.")
                    break
    except KeyboardInterrupt:
        print("\nПроцесс остановлен пользователем (Ctrl+C).")
    except Exception as e:
        print(f"\nОшибка при работе профиля: {e}")

if __name__ == "__main__":
    profile_name = sys.argv[1] if len(sys.argv) > 1 else "sanyaromaha@mail.ru"
    url = sys.argv[2] if len(sys.argv) > 2 else "https://e.mail.ru/inbox/"
    launch_expert_profile(profile_name, target_url=url)
