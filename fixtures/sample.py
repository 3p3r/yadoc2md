from dataclasses import dataclass
from typing import List

@dataclass
class User:
    id: int
    name: str
    email: str

def greet(user: User) -> str:
    return f"Hello, {user.name}!"

users: List[User] = [
    User(1, "Alice", "alice@example.com"),
    User(2, "Bob", "bob@example.com"),
]

for user in users:
    print(greet(user))
