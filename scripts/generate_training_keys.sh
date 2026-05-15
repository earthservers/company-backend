#!/bin/bash
set -e

KEYS_DIR="/opt/company/keys"
mkdir -p "$KEYS_DIR"

echo "Generating training encryption keys..."

# Generate RSA-4096 key pair for client->server encryption (Layer 1)
openssl genrsa -out "$KEYS_DIR/training_private.pem" 4096
openssl rsa -in "$KEYS_DIR/training_private.pem" -pubout -out "$KEYS_DIR/training_public.pem"

# Generate AES-256 master key for database encryption (Layer 2)
openssl rand -hex 32 > "$KEYS_DIR/db_master_key.txt"

# Secure permissions
chmod 600 "$KEYS_DIR/training_private.pem"
chmod 600 "$KEYS_DIR/db_master_key.txt"
chmod 644 "$KEYS_DIR/training_public.pem"

echo "Keys generated successfully"
echo "  Private key: $KEYS_DIR/training_private.pem"
echo "  Public key:  $KEYS_DIR/training_public.pem"
echo "  DB master key: $KEYS_DIR/db_master_key.txt"
echo ""
echo "Add to Revolt.toml [training] section:"
echo "  private_key_path = \"$KEYS_DIR/training_private.pem\""
echo "  public_key_path = \"$KEYS_DIR/training_public.pem\""
echo "  db_master_key = \"$(cat "$KEYS_DIR/db_master_key.txt")\""
