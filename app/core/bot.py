# app/core/bot.py

import pickle
import socket
import atexit

KNOWN_BOTS = {
    "Googlebot": [".googlebot.com"],
    "Bingbot": [".search.msn.com"],
    "AhrefsBot": [".ahrefs.com", ".ahrefs.net"],
    "YandexBot": [".yandex.ru", ".yandex.com", ".yandex.net"],
    "SemrushBot": [".semrush.com"],
    "DuckDuckBot": [".duckduckgo.com"],
    "MJ12bot": [".majestic12.co.uk"],
    "Slurp": [".crawl.yahoo.net"],
    "Applebot": [".apple.com"]
}

VERIFIED_IP_FILE = "verified_bots.pkl"
SPOOFED_IP_FILE = "spoofed_bots.pkl"


def pickle_load(path):
    try:
        with open(path, "rb") as f:
            return pickle.load(f)
    except Exception:
        return set()


verified_bot_ips = pickle_load(VERIFIED_IP_FILE)
spoofed_bot_ips = pickle_load(SPOOFED_IP_FILE)


def save_ip_caches():
    with open(VERIFIED_IP_FILE, "wb") as f:
        pickle.dump(verified_bot_ips, f)
    with open(SPOOFED_IP_FILE, "wb") as f:
        pickle.dump(spoofed_bot_ips, f)


atexit.register(save_ip_caches)


def reverse_dns(ip):
    try:
        return socket.gethostbyaddr(ip)[0]
    except socket.herror:
        return None


def forward_dns(hostname):
    try:
        return socket.gethostbyname(hostname)
    except socket.gaierror:
        return None


def is_valid_bot(ip, ua):
    if ip in verified_bot_ips:
        return True
    if ip in spoofed_bot_ips:
        return False

    for bot, suffixes in KNOWN_BOTS.items():
        if bot.lower() in ua.lower():
            rdns = reverse_dns(ip)
            if not rdns or not any(rdns.endswith(sfx) for sfx in suffixes):
                spoofed_bot_ips.add(ip)
                return False
            if forward_dns(rdns) != ip:
                spoofed_bot_ips.add(ip)
                return False
            verified_bot_ips.add(ip)
            return True

    return False
