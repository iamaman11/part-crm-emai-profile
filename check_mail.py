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

def get_profile_config(profile_dir: str):
    config_path = os.path.join(profile_dir, "camoufox_config.json")
    if os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass
    return None

def check_inbox(profile_name: str, sender_filter: str = None, unread_only: bool = False, headless: bool = False):
    profile_dir = os.path.expanduser(f"~/browser_profiles/{profile_name}")
    cleanup_stale_locks(profile_dir)
    config_data = get_profile_config(profile_dir)

    print(f"\n============================================================")
    print(f"[CheckMail] Сканирование ящика: {profile_name}")
    if sender_filter:
        print(f"[CheckMail] Фильтр по отправителю: '{sender_filter}'")
    print(f"[CheckMail] Только новые (непрочитанные): {unread_only}")
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
            
            print("Переходим на https://e.mail.ru/inbox/ ...")
            page.goto("https://e.mail.ru/inbox/", wait_until="domcontentloaded", timeout=60000)
            
            # Небольшая задержка для загрузки списка
            time.sleep(5)

            # Селекторы писем в Mail.ru
            letter_elements = page.query_selector_all("a.llc, a.dataset-letter, .letter-list-item")
            
            letters = []
            for el in letter_elements:
                sender_el = el.query_selector(".llc__item_title, .llc__name, .ll-cr, .letter-contact")
                sender = sender_el.inner_text().strip() if sender_el else ""
                if not sender:
                    sender = el.get_attribute("title") or "Неизвестен"
                
                subject_el = el.query_selector(".llc__subject, .ll-sj, .letter-subject")
                subject = subject_el.inner_text().strip() if subject_el else "Без темы"
                
                date_el = el.query_selector(".llc__date, .ll-dt, .letter-date")
                date_str = date_el.inner_text().strip() if date_el else ""
                
                class_attr = el.get_attribute("class") or ""
                read_attr = el.get_attribute("data-read") or ""
                is_unread = ("llc_unread" in class_attr) or ("unread" in class_attr) or (read_attr == "false")

                if unread_only and not is_unread:
                    continue
                if sender_filter and (sender_filter.lower() not in sender.lower() and sender_filter.lower() not in subject.lower()):
                    continue

                letters.append({
                    "sender": sender,
                    "subject": subject,
                    "date": date_str,
                    "unread": is_unread
                })

            unread_count = sum(1 for l in letters if l["unread"])
            print("\n------------------------------------------------------------")
            print(f"[CheckMail] Результат сканирования: всего писем в списке {len(letters)} (из них новых/непрочитанных: {unread_count})")
            print("------------------------------------------------------------")
            if not letters:
                print(">>> Новых/совпадающих писем не обнаружено <<<")
            else:
                for idx, l in enumerate(letters, 1):
                    status_icon = "🔴 [НОВОЕ ПИСЬМО]" if l["unread"] else "⚪ [ПРОЧИТАНО]"
                    print(f"{idx}. {status_icon} От: {l['sender']} | Тема: {l['subject']} ({l['date']})")
            print("------------------------------------------------------------\n")
            
            return letters
    except Exception as e:
        print(f"[CheckMail] Ошибка при сканировании входящих: {e}")
        return []

if __name__ == "__main__":
    profile = sys.argv[1] if len(sys.argv) > 1 else "vip.missolga1988@mail.ru"
    sender = sys.argv[2] if len(sys.argv) > 2 else None
    check_inbox(profile_name=profile, sender_filter=sender, unread_only=False, headless=False)
