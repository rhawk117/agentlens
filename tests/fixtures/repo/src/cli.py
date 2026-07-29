import sys

from .api.users import UserService


def main(argv: list[str] | None = None) -> int:
    service = UserService()
    service.create_user("a@example.com", "A")
    return 0


if __name__ == "__main__":
    sys.exit(main())
