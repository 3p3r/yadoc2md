interface User {
  id: number
  name: string
  email: string
}

function greet(user: User): string {
  return `Hello, ${user.name}!`
}

const user: User = { id: 1, name: "Alice", email: "alice@example.com" }
console.log(greet(user))
