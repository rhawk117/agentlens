import re

TIMEOUT_SECONDS = 30
RETRY_LIMIT = 3
DSN = "postgres://localhost:5432/app"
SLUG_PATTERN = re.compile(r"^[a-z0-9-]+$")


def describe(host: str, port: int) -> str:
    return f"connection refused: {host}:{port}"


def banner() -> str:
    return "connection refused: nowhere"
