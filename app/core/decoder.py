# app/core/decoder.py

import os
import re
import codecs
import urllib.parse
import base64
import polars as pl
from .bot import is_valid_bot

log_pattern = re.compile(
    r'(?P<ip>\S+) - - \[(?P<time>[^\]]+)\] '
    r'"(?P<method>\S+) (?P<url>\S+) (?P<protocol>[^\"]+)" '
    r'(?P<status>\d+) (?P<size>\d+) '
    r'"(?P<referrer>[^\"]*)" "(?P<user_agent>[^\"]*)" "(?P<extra>[^\"]*)"'
)


def esc_nl(text):
    return text.replace('\n', '\\n').replace('\r', '\\r').strip()


def dec_url(text):
    try:
        first = urllib.parse.unquote(text)
        if first == text:
            return text
        second = urllib.parse.unquote(first)
        if second == first:
            return first
        return second
    except Exception:
        return text


def dec_esc(text):
    try:
        if '\\x' in text or '\\u' in text:
            decoded = codecs.escape_decode(text.encode())[0].decode('utf-8', errors='replace')
            return decoded
        return text
    except Exception:
        return text


def dec_base64(text):
    try:
        last_part = text.rsplit("/", 1)[-1]
        if re.fullmatch(r'[A-Za-z0-9+/=]{8,}', last_part):
            decoded = base64.b64decode(last_part, validate=False).decode('utf-8', errors='ignore')
            annotated = f"{text}(base64:{decoded})"
            return annotated
        return text
    except Exception:
        return text


def dec_combined(text):
    text = dec_url(text)
    text = dec_esc(text)
    text = dec_base64(text)
    return text


def parse_dec_line(line):
    match = log_pattern.match(line)
    if not match:
        return None, None
    fields = match.groupdict()
    fields['url'] = dec_combined(fields['url'])
    fields['referrer'] = dec_combined(fields['referrer'])
    for key in fields:
        fields[key] = esc_nl(fields[key])
    decoded = (
        f"{fields['ip']} - - [{fields['time']}] "
        f'"{fields['method']} {fields['url']} {fields['protocol']}" '
        f"{fields['status']} {fields['size']} "
        f'"{fields['referrer']}" "{fields['user_agent']}" "{fields['extra']}"'
    )
    return decoded, fields


def parse_dec_file_to_dataframe(in_path):
    with open(in_path, 'r', encoding='utf-8', errors='replace') as in_file:
        records = []
        for no, line in enumerate(in_file, 1):
            _, fields = parse_dec_line(line)
            if not fields:
                continue
            if is_valid_bot(fields['ip'], fields['user_agent']):
                continue
            fields['no'] = no
            records.append(fields)

    df = pl.DataFrame(records)
    # Parse time column into datetime where possible
    try:
        df = df.with_columns(
            pl.col('time').str.strptime(pl.Datetime, fmt='%d/%b/%Y:%H:%M:%S %z', strict=False).alias('time')
        )
    except Exception:
        # Fallback: keep as string if parsing fails
        pass

    df = df.with_columns([
        pl.col('status').cast(pl.Int32),
        pl.col('size').cast(pl.Int32),
    ])
    return df


def parse_dec_file(in_path, out_path):
    with open(in_path, 'r', encoding='utf-8', errors='replace') as in_file, open(out_path, 'w', encoding='utf-8') as out_file:
        for line in in_file:
            decoded, fields = parse_dec_line(line)
            if not fields:
                continue
            if is_valid_bot(fields['ip'], fields['user_agent']):
                continue
            out_file.write(f"{decoded}\n")


def parse_dec_file_to_csv(in_path, out_path):
    # Backwards-compatible API: write clustering/parsing output as Parquet using Polars
    df = parse_dec_file_to_dataframe(in_path)
    # Ensure parent dir exists
    parent = os.path.dirname(out_path)
    if parent and not os.path.exists(parent):
        os.makedirs(parent, exist_ok=True)
    # Write parquet (replace previous CSV behavior)
    try:
        df.write_parquet(out_path)
    except Exception:
        # If a .csv path was provided, replace extension with .parquet
        base, _ = os.path.splitext(out_path)
        pq_path = f"{base}.parquet"
        df.write_parquet(pq_path)
