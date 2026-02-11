#!/bin/bash
# PostgreSQL Native Setup for Ubuntu/Debian

echo "Installing PostgreSQL..."

# Install PostgreSQL
sudo apt update
sudo apt install -y postgresql postgresql-contrib

# Start PostgreSQL
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Create database and user
sudo -u postgres psql << EOF
CREATE DATABASE wallet_db;
CREATE USER wallet_user WITH PASSWORD 'wallet_pass';
GRANT ALL PRIVILEGES ON DATABASE wallet_db TO wallet_user;
\c wallet_db
GRANT ALL ON SCHEMA public TO wallet_user;
EOF

echo "PostgreSQL setup complete!"
echo "Connection string: postgres://wallet_user:wallet_pass@localhost:5432/wallet_db"