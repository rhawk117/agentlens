from src.api.users import UserService


def test_create_user():
    service = UserService()
    user = service.create_user("a@example.com", "A")
    assert user.email == "a@example.com"


def test_delete_user():
    service = UserService()
    service.delete_user(1)
