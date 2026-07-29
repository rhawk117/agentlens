"""User service."""

from typing import overload

MAX_RETRIES = 3
DEFAULT_ROLE = "member"


class User:
    def __init__(self, email: str, name: str) -> None:
        self.email = email
        self.name = name


class UserService:
    """Creates and removes users."""

    RETRY_BACKOFF = 2

    # This comment must never appear in the slice.
    @transaction.atomic
    @audit("user.create")
    def create_user(self, email: str, name: str) -> User:
        if not email:
            raise ValueError("email is required")
        user = User(email, name)
        self._log("created user %s", email)
        return user

    def delete_user(self, uid: int) -> None:
        self._log("deleting user %d", uid)
        self._store.pop(uid, None)

    def _log(self, template: str, *args: object) -> None:
        print(template % args)

    class Cursor:
        def next(self) -> int:
            return 1


@overload
def normalise(value: int) -> int: ...


@overload
def normalise(value: str) -> str: ...


@overload
def normalise(value: bytes) -> bytes: ...


def normalise(value):
    if isinstance(value, bytes):
        return value.strip()
    return value


if TYPE_CHECKING:

    def only_when_typing() -> None:
        pass
