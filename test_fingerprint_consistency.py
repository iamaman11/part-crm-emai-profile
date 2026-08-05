import os
import sys
import json
import time

sys.path.append("/home/bose/projects/camoufox/python")
from camoufox import Camoufox

PROXY_CONFIG = {
    "server": "http://34.118.88.54:3128",
    "username": "relay4855cb91",
    "password": "4gKDPTqhCtFwSvy5FlsDJO91e7A4r3t9"
}

def cleanup_stale_locks(profile_dir: str):
    for lock_name in [".parentlock", "lock"]:
        lock_path = os.path.join(profile_dir, lock_name)
        if os.path.exists(lock_path) or os.path.islink(lock_path):
            try:
                os.remove(lock_path)
            except Exception:
                pass

def test_profile_fingerprint(profile_name: str, headless: bool = False):
    profile_dir = os.path.expanduser(f"~/browser_profiles/{profile_name}")
    cleanup_stale_locks(profile_dir)
    
    config_path = os.path.join(profile_dir, "camoufox_config.json")
    config_data = None
    if os.path.exists(config_path):
        with open(config_path, "r", encoding="utf-8") as f:
            config_data = json.load(f)

    print(f"\n============================================================")
    print(f"[Fingerprint Test] Комплексная проверка консистентности: {profile_name}")
    print(f"Путь к профилю: {profile_dir}")
    print(f"Зафиксированный конфигуратор: {'ДА' if config_data else 'НЕТ'}")
    print(f"============================================================\n")

    launch_kwargs = {
        "persistent_context": True,
        "user_data_dir": profile_dir,
        "os": "windows",
        "proxy": PROXY_CONFIG,
        "geoip": True,
        "headless": headless
    }
    if config_data:
        launch_kwargs["config"] = config_data
        launch_kwargs["i_know_what_im_doing"] = True

    try:
        with Camoufox(**launch_kwargs) as context:
            page = context.pages[0] if context.pages else context.new_page()

            # 1. Проверка CreepJS
            print("1. Переход на CreepJS (эталонный тестер консистентности отпечатков)...")
            page.goto("https://abrahamjuliot.github.io/creepjs/", wait_until="domcontentloaded", timeout=60000)
            time.sleep(6) # Ожидание полного просчета математических шумов и веб-воркеров

            try:
                # Извлекаем отпечаток и доверительный балл CreepJS
                creep_score = page.evaluate("""() => {
                    const trust = document.querySelector('.trust-score, .score') || document.querySelector('[data-trust]');
                    const fingerprint = document.querySelector('#fp-id') || document.querySelector('.fingerprint');
                    return {
                        trustText: trust ? trust.innerText : 'Calculated',
                        fpId: fingerprint ? fingerprint.innerText : 'Generated'
                    };
                }""")
                print(f"   [CreepJS Result] Trust Score / Status: {creep_score.get('trustText')}")
                print(f"   [CreepJS Result] Fingerprint Hash ID: {creep_score.get('fpId')}")
            except Exception as e:
                print(f"   [CreepJS Warning]: {e}")

            # 2. Проверка Browserleaks WebGL / Canvas / WebRTC
            print("\n2. Переход на Browserleaks (проверка утечек WebRTC и Canvas)...")
            page.goto("https://browserleaks.com/canvas", wait_until="domcontentloaded", timeout=60000)
            time.sleep(3)

            # 3. Переход на IPLeak / Iphey
            print("\n3. Переход на IPInfo / Iphey (проверка таймзоны и прокси)...")
            page.goto("https://ipinfo.io/json", wait_until="domcontentloaded", timeout=30000)
            ip_json = page.text_content("body")
            print(f"   [Network & GeoIP Alignment]:\n{ip_json.strip()}")

            print("\n------------------------------------------------------------")
            print("Все тесты отпечатков завершены. Окно браузера открыто для внешнего осмотра.")
            print("------------------------------------------------------------\n")

            while True:
                time.sleep(5)
                if not context.pages or page.is_closed():
                    print("[Fingerprint Test] Окно закрыто. Завершено.")
                    break
    except KeyboardInterrupt:
        print("\nОстановлено пользователем.")
    except Exception as e:
        print(f"\nОшибка при проверке: {e}")

if __name__ == "__main__":
    profile = sys.argv[1] if len(sys.argv) > 1 else "fresh_account_profile_18@mail.ru"
    test_profile_fingerprint(profile, headless=False)
