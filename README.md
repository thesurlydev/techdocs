# sbfly - Spring Boot on Fly.io

A modern Spring Boot application template designed for deployment on Fly.io, featuring OAuth2 authentication with Google and session management using PostgreSQL.

## Key Features

- OAuth2 authentication with Google
- Session management with JDBC
- JTE templating engine
- DaisyUI + Tailwind CSS for styling
- Dark/Light theme support
- Docker multi-stage build
- Virtual threads enabled
- Fly.io deployment ready

## Prerequisites

- Java 23
- Maven
- PostgreSQL
- Google OAuth2 credentials
- Fly.io account

## Installation

1. Clone the repository:
```bash
git clone https://github.com/thesurlydev/sbfly.git
```

2. Configure environment variables:
```properties
GOOGLE_CLIENT_ID=your_client_id
GOOGLE_CLIENT_SECRET=your_client_secret
DB_URL=jdbc:postgresql://localhost:5432/your_db
DB_USERNAME=your_username
DB_PASSWORD=your_password
```

## Usage

Build and run locally:
```bash
./mvnw clean package
java -jar target/sbfly-0.0.1-SNAPSHOT.jar
```

Using Just:
```bash
# Build project
just build

# Build and run Docker image
just run

# Deploy to Fly.io
just deploy
```

## Project Structure

```
src/
├── main/
│   ├── java/
│   │   └── dev/surly/sbfly/
│   │       ├── security/       # Security configuration
│   │       ├── user/          # User management
│   │       └── controllers/   # Web controllers
│   ├── jte/                  # JTE templates
│   │   ├── components/      # Reusable UI components
│   │   ├── layout/         # Page layouts
│   │   └── pages/         # Page templates
│   └── resources/
│       ├── application.properties
│       └── schema.sql
```