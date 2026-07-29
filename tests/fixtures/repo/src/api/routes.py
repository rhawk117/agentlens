from .users import UserService

app = Router()
service = UserService()


@app.get("/health")
def health() -> dict:
    return {"status": "ok"}


@app.post("/users")
def create(email: str, name: str) -> dict:
    user = service.create_user(email, name)
    return {"email": user.email}


def _unused_helper(value: str) -> str:
    return value.strip()
