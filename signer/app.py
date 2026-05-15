#!/usr/bin/env python3
"""
Minimal Flask mock signer implementing the OpenAPI endpoints used by the scaffold.
NOT FOR PRODUCTION.

- GET /health
- POST /token  (requires X-API-Key header)
- POST /token/<token>/claim  (requires X-Token header)

This simple server stores claims in ./live/claims.json and writes per-token files under ./live/.
"""
import os
import sys
import json
import secrets
import uuid
import datetime
import base64
import sqlite3
import smtplib
import hmac
import time
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError
from flask import Flask, request, jsonify, abort
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import padding, ec, rsa
from cryptography.hazmat.primitives.asymmetric import utils as asym_utils
from cryptography.hazmat.primitives import hashes as primitive_hashes
from cryptography.exceptions import InvalidSignature
import logging
logging.basicConfig(level=logging.INFO)

app = Flask(__name__)


@app.after_request
def add_cors_headers(response):
    response.headers['Access-Control-Allow-Origin'] = '*'
    response.headers['Access-Control-Allow-Headers'] = 'Content-Type, X-API-Key, X-Token, X-Session-Token, X-Key-Id, X-Key-Password'
    response.headers['Access-Control-Allow-Methods'] = 'GET, POST, OPTIONS'
    return response
BASE_DIR = os.environ.get('DATA_DIR', '/app/live')
ADMIN_API_KEY = os.environ.get('ADMIN_API_KEY', 'changeme')
TOKEN_TTL_SECONDS = int(os.environ.get('TOKEN_TTL_SECONDS', '900'))  # 15 minutes

# --- SMTP configuration ---
SMTP_HOST = os.environ.get('SMTP_HOST', 'localhost')
SMTP_PORT = int(os.environ.get('SMTP_PORT', '14025'))
SMTP_FROM = os.environ.get('SMTP_FROM', 'noreply@earthservers.net')
SMTP_USERNAME = os.environ.get('SMTP_USERNAME', '')
SMTP_PASSWORD = os.environ.get('SMTP_PASSWORD', '')

# --- Revolt API for session verification ---
REVOLT_API_URL = os.environ.get('REVOLT_API_URL', 'http://localhost:14702')

# --- Email MFA settings ---
EMAIL_MFA_CODE_TTL = 600       # 10 minutes
EMAIL_MFA_MAX_ATTEMPTS = 5
EMAIL_MFA_RATE_LIMIT = 3       # max codes per 15 minutes
EMAIL_MFA_RATE_WINDOW = 900    # 15 minutes

os.makedirs(BASE_DIR, exist_ok=True)
CLAIMS_FILE = os.path.join(BASE_DIR, 'claims.json')
TOKENS_FILE = os.path.join(BASE_DIR, 'tokens.json')
DB_FILE = os.path.join(BASE_DIR, 'data.db')


# --- SQLite helpers ---
def get_conn():
    # Each call returns a fresh connection; we use explicit transactions where needed.
    conn = sqlite3.connect(DB_FILE, timeout=5.0)
    conn.row_factory = sqlite3.Row
    return conn


def init_db():
    conn = get_conn()
    cur = conn.cursor()
    cur.execute('''
    CREATE TABLE IF NOT EXISTS tokens (
        token TEXT PRIMARY KEY,
        action TEXT,
        userId TEXT,
        expiresAt TEXT,
        expiresTs REAL,
        used INTEGER DEFAULT 0,
        password TEXT,
        createdAt TEXT,
        maxClaims INTEGER DEFAULT 1,
        claimCount INTEGER DEFAULT 0
    )
    ''')
    cur.execute('''
    CREATE TABLE IF NOT EXISTS claims (
        keyId TEXT PRIMARY KEY,
        publicKeyPem TEXT,
        proofSignature TEXT,
        metadata TEXT,
        token TEXT,
        createdAt TEXT,
        FOREIGN KEY(token) REFERENCES tokens(token)
    )
    ''')
    cur.execute('''
    CREATE TABLE IF NOT EXISTS email_mfa (
        user_id TEXT PRIMARY KEY,
        enabled INTEGER DEFAULT 0,
        created_at TEXT
    )
    ''')
    cur.execute('''
    CREATE TABLE IF NOT EXISTS email_mfa_codes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id TEXT,
        code TEXT,
        expires_at REAL,
        attempts INTEGER DEFAULT 0,
        used INTEGER DEFAULT 0,
        created_at TEXT
    )
    ''')
    conn.commit()
    conn.close()


def migrate_json_to_sqlite():
    # If JSON files exist from previous runs, import them to SQLite to preserve state.
    if not os.path.exists(TOKENS_FILE) and not os.path.exists(CLAIMS_FILE):
        return
    try:
        tokens = {}
        claims = {}
        if os.path.exists(TOKENS_FILE):
            with open(TOKENS_FILE, 'r') as f:
                tokens = json.load(f)
        if os.path.exists(CLAIMS_FILE):
            with open(CLAIMS_FILE, 'r') as f:
                claims = json.load(f)

        conn = get_conn()
        cur = conn.cursor()
        for t, v in (tokens or {}).items():
            cur.execute('''INSERT OR REPLACE INTO tokens(token, action, userId, expiresAt, expiresTs, used, password, createdAt, maxClaims, claimCount)
                           VALUES(?,?,?,?,?,?,?,?,?,?)''', (
                v.get('token'), v.get('action'), v.get('userId'), v.get('expiresAt'), v.get('expiresTs'), 1 if v.get('used') else 0,
                v.get('password'), v.get('createdAt'), v.get('maxClaims', 1), v.get('claimCount', 0)
            ))
        for k, c in (claims or {}).items():
            cur.execute('''INSERT OR REPLACE INTO claims(keyId, publicKeyPem, proofSignature, metadata, token, createdAt)
                           VALUES(?,?,?,?,?,?)''', (
                c.get('keyId'), c.get('publicKeyPem'), c.get('proofSignature'), json.dumps(c.get('metadata') or {}), c.get('token'), c.get('createdAt')
            ))
        conn.commit()
        conn.close()
        # leave JSON files untouched for debugging, but they are now imported
    except Exception:
        logging.exception('Failed to migrate JSON to SQLite')


# simple helper to load and persist claims
def _row_to_token(row):
    if not row:
        return None
    return {
        'token': row['token'],
        'action': row['action'],
        'userId': row['userId'],
        'expiresAt': row['expiresAt'],
        'expiresTs': row['expiresTs'],
        'used': bool(row['used']),
        'password': row['password'],
        'createdAt': row['createdAt'],
        'maxClaims': row['maxClaims'],
        'claimCount': row['claimCount'],
    }


def get_token_record(token):
    conn = get_conn()
    row = conn.execute('SELECT * FROM tokens WHERE token=?', (token,)).fetchone()
    conn.close()
    return _row_to_token(row)


def create_token_record(t, action, userId, expiresAt, expiresTs, password, createdAt, max_claims):
    conn = get_conn()
    conn.execute('''INSERT OR REPLACE INTO tokens(token, action, userId, expiresAt, expiresTs, used, password, createdAt, maxClaims, claimCount)
                    VALUES(?,?,?,?,?,?,?,?,?,?)''', (t, action, userId, expiresAt, expiresTs, 0, password, createdAt, max_claims, 0))
    conn.commit()
    conn.close()


def insert_claim_and_update_token(entry):
    # entry: dict with keyId, publicKeyPem, proofSignature, metadata, token, createdAt
    conn = get_conn()
    cur = conn.cursor()
    try:
        cur.execute('BEGIN IMMEDIATE')
        cur.execute('INSERT INTO claims(keyId, publicKeyPem, proofSignature, metadata, token, createdAt) VALUES(?,?,?,?,?,?)', (
            entry['keyId'], entry['publicKeyPem'], entry['proofSignature'], json.dumps(entry.get('metadata') or {}), entry['token'], entry['createdAt']
        ))
        cur.execute('UPDATE tokens SET claimCount = claimCount + 1 WHERE token = ?', (entry['token'],))
        row = cur.execute('SELECT claimCount, maxClaims FROM tokens WHERE token = ?', (entry['token'],)).fetchone()
        claim_count = row['claimCount'] if row else 0
        max_claims = row['maxClaims'] if row else 1
        if claim_count >= max_claims:
            cur.execute('UPDATE tokens SET used = 1 WHERE token = ?', (entry['token'],))
        conn.commit()
        return True
    except Exception:
        conn.rollback()
        logging.exception('Failed to insert claim and update token atomically')
        return False
    finally:
        conn.close()


def now_iso():
    return datetime.datetime.utcnow().isoformat() + 'Z'


@app.route('/health', methods=['GET'])
def health():
    return jsonify({'status': 'ok', 'time': now_iso()})


@app.route('/token', methods=['POST'])
def token():
    api_key = request.headers.get('X-API-Key')
    if api_key != ADMIN_API_KEY:
        return jsonify({'error': 'unauthorized'}), 401
    body = request.get_json(force=True, silent=True) or {}
    action = body.get('action', 'sign')
    userId = body.get('userId')
    max_claims = int(body.get('maxClaims', 1))
    # optional password: admin may provide a password to be associated with this token
    password = body.get('password')
    t = str(uuid.uuid4())
    expires_ts = (datetime.datetime.utcnow() + datetime.timedelta(seconds=TOKEN_TTL_SECONDS)).timestamp()
    expires = datetime.datetime.utcfromtimestamp(expires_ts).isoformat() + 'Z'

    # if no password supplied, generate a random URL-safe token for E2E use
    if not password:
        password = secrets.token_urlsafe(16)

    create_token_record(t, action, userId, expires, expires_ts, password, now_iso(), max_claims)

    resp = {'token': t, 'expiresAt': expires, 'action': action, 'userId': userId, 'ttlSeconds': TOKEN_TTL_SECONDS, 'password': password, 'maxClaims': max_claims}
    return jsonify(resp), 201


def verify_signature(public_pem: str, signature_b64: str, message: bytes) -> bool:
    try:
        pub = serialization.load_pem_public_key(public_pem.encode('utf-8'))
    except Exception:
        logging.exception('Failed to load public key PEM')
        return False

    try:
        sig = base64.b64decode(signature_b64)
    except Exception:
        return False

    # Try RSA PKCS1v15 / PSS or ECDSA depending on key type
    try:
        # Detect key type using cryptography classes
        if isinstance(pub, rsa.RSAPublicKey):
            # RSA: first try PKCS1v15 with SHA256
            try:
                pub.verify(sig, message, padding.PKCS1v15(), hashes.SHA256())
                return True
            except InvalidSignature:
                logging.info('RSA PKCS1v15 verification failed, trying PSS')
                # Try PSS
                try:
                    pub.verify(sig, message, padding.PSS(mgf=padding.MGF1(hashes.SHA256()), salt_length=padding.PSS.MAX_LENGTH), hashes.SHA256())
                    return True
                except InvalidSignature:
                    logging.info('PSS also failed — attempting Prehashed PKCS1v15 fallback')
                    # Some tooling (openssl dgst -sign) may produce signatures over the digest directly.
                    try:
                        digest = primitive_hashes.Hash(primitive_hashes.SHA256())
                        digest.update(message)
                        pre = digest.finalize()
                        pub.verify(sig, pre, padding.PKCS1v15(), asym_utils.Prehashed(primitive_hashes.SHA256()))
                        return True
                    except Exception:
                        logging.exception('RSA verification failed for PKCS1v15, PSS and Prehashed fallback')
                        return False
        elif isinstance(pub, ec.EllipticCurvePublicKey):
            pub.verify(sig, message, ec.ECDSA(hashes.SHA256()))
            return True
        else:
            # Unknown key type
            return False
    except Exception:
        logging.exception('Unexpected error during signature verification')
        # Some key types might raise different errors, treat as invalid
        return False

    return False


@app.route('/token/<token>/claim', methods=['POST'])
def claim(token):
    token_hdr = request.headers.get('X-Token')
    if token_hdr != token:
        return jsonify({'error': 'invalid token header'}), 401

    tok = get_token_record(token)
    if not tok:
        return jsonify({'error': 'unknown token'}), 404

    # check expiry / used
    if tok.get('used'):
        return jsonify({'error': 'token already used'}), 400
    if datetime.datetime.utcnow().timestamp() > tok.get('expiresTs', 0):
        return jsonify({'error': 'token expired'}), 400

    # Log raw request for debugging
    try:
        raw = request.get_data(as_text=True)
        logging.info('Raw request body: %s', raw)
        logging.info('Request headers: %s', dict(request.headers))
    except Exception:
        logging.exception('Failed to read raw request')

    body = request.get_json(force=True, silent=True) or {}
    publicKeyPem = body.get('publicKeyPem')
    proofSignature = body.get('proofSignature')
    metadata = body.get('metadata')
    if not publicKeyPem or not proofSignature:
        return jsonify({'error': 'missing fields'}), 400

    # The expected message to sign is the token value (utf-8 bytes)
    message = token.encode('utf-8')

    # Debug: print brief info for troubleshooting signature failures
    try:
        print(f"DEBUG: received publicKeyPem len={len(publicKeyPem)} proofSignature_len={len(proofSignature)}", file=sys.stderr)
        print(f"DEBUG: publicKeyPem head={publicKeyPem[:120]!r}", file=sys.stderr)
    except Exception:
        pass

    ok = verify_signature(publicKeyPem, proofSignature, message)
    if not ok:
        return jsonify({'error': 'invalid signature'}), 400

    key_id = str(uuid.uuid4())
    entry = {
        'keyId': key_id,
        'publicKeyPem': publicKeyPem,
        'proofSignature': proofSignature,
        'metadata': metadata or {},
        'token': token,
        'createdAt': now_iso(),
    }

    ok = insert_claim_and_update_token(entry)
    if not ok:
        return jsonify({'error': 'failed to persist claim'}), 500

    # also write individual file for quick inspection
    try:
        token_dir = os.path.join(BASE_DIR, token)
        os.makedirs(token_dir, exist_ok=True)
        with open(os.path.join(token_dir, f'{key_id}.json'), 'w') as f:
            json.dump(entry, f, indent=2)
    except Exception:
        logging.exception('Failed to write per-token claim file')

    # Return password so claimants can use it for pairing if needed
    tok = get_token_record(token)
    return jsonify({'result': 'accepted', 'keyId': key_id, 'password': tok.get('password')}), 200


@app.route('/token/<token>/claims', methods=['GET'])
def list_token_claims(token):
    # Authenticate via EITHER admin API key OR claim credentials (X-Key-Id + X-Key-Password)
    api_key = request.headers.get('X-API-Key')
    key_id = request.headers.get('X-Key-Id')
    key_password = request.headers.get('X-Key-Password')

    authorized = False
    if api_key == ADMIN_API_KEY:
        authorized = True
    elif key_id and key_password:
        # Verify that key_id belongs to a claim on this token and password matches the token's password
        tok = get_token_record(token)
        if tok and tok.get('password') == key_password:
            conn = get_conn()
            row = conn.execute('SELECT 1 FROM claims WHERE keyId=? AND token=?', (key_id, token)).fetchone()
            conn.close()
            if row:
                authorized = True

    if not authorized:
        return jsonify({'error': 'unauthorized'}), 401

    tok = get_token_record(token)
    if not tok:
        return jsonify({'error': 'unknown token'}), 404

    conn = get_conn()
    rows = conn.execute('SELECT * FROM claims WHERE token = ? ORDER BY createdAt', (token,)).fetchall()
    result = []
    for r in rows:
        result.append({
            'createdAt': r['createdAt'],
            'keyId': r['keyId'],
            'metadata': json.loads(r['metadata'] or '{}'),
            'proofSignature': r['proofSignature'],
            'publicKeyPem': r['publicKeyPem'],
            'token': r['token'],
        })
    conn.close()
    return jsonify({'token': token, 'claims': result, 'claimCount': tok.get('claimCount', 0)}), 200


# =============================================
# Email MFA helpers
# =============================================

def verify_session(session_token):
    """Verify a session token against the Revolt API and return user info."""
    try:
        req = Request(
            REVOLT_API_URL + '/auth/account/',
            headers={'X-Session-Token': session_token},
            method='GET',
        )
        with urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            return {'user_id': data.get('_id'), 'email': data.get('email')}
    except HTTPError as e:
        logging.warning('Session verification failed: HTTP %s', e.code)
        return None
    except (URLError, Exception) as e:
        logging.exception('Session verification error')
        return None


def send_email(to_addr, subject, body):
    """Send a plain-text email via SMTP."""
    msg = MIMEMultipart()
    msg['From'] = SMTP_FROM
    msg['To'] = to_addr
    msg['Subject'] = subject
    msg.attach(MIMEText(body, 'plain'))

    try:
        smtp = smtplib.SMTP(SMTP_HOST, SMTP_PORT, timeout=10)
        if SMTP_USERNAME and SMTP_PASSWORD:
            smtp.login(SMTP_USERNAME, SMTP_PASSWORD)
        smtp.sendmail(SMTP_FROM, [to_addr], msg.as_string())
        smtp.quit()
        logging.info('Email sent to %s', to_addr)
        return True
    except Exception:
        logging.exception('Failed to send email to %s', to_addr)
        return False


def generate_mfa_code():
    """Generate a cryptographically random 6-digit code."""
    return str(secrets.randbelow(900000) + 100000)


def check_rate_limit(user_id):
    """Check if user has exceeded rate limit for code requests."""
    conn = get_conn()
    cutoff = time.time() - EMAIL_MFA_RATE_WINDOW
    row = conn.execute(
        'SELECT COUNT(*) as cnt FROM email_mfa_codes WHERE user_id = ? AND created_at > ?',
        (user_id, datetime.datetime.utcfromtimestamp(cutoff).isoformat() + 'Z')
    ).fetchone()
    conn.close()
    return row['cnt'] < EMAIL_MFA_RATE_LIMIT


def store_mfa_code(user_id, code):
    """Store an MFA code in the database."""
    conn = get_conn()
    expires_at = time.time() + EMAIL_MFA_CODE_TTL
    conn.execute(
        'INSERT INTO email_mfa_codes (user_id, code, expires_at, attempts, used, created_at) VALUES (?, ?, ?, 0, 0, ?)',
        (user_id, code, expires_at, now_iso())
    )
    conn.commit()
    conn.close()


def verify_mfa_code(user_id, code):
    """Verify an MFA code. Returns (success, error_message)."""
    conn = get_conn()
    cur = conn.cursor()
    try:
        cur.execute('BEGIN IMMEDIATE')
        row = cur.execute(
            'SELECT id, code, expires_at, attempts, used FROM email_mfa_codes '
            'WHERE user_id = ? AND used = 0 ORDER BY id DESC LIMIT 1',
            (user_id,)
        ).fetchone()

        if not row:
            conn.rollback()
            return False, 'no_pending_code'

        if row['used']:
            conn.rollback()
            return False, 'code_already_used'

        if time.time() > row['expires_at']:
            cur.execute('UPDATE email_mfa_codes SET used = 1 WHERE id = ?', (row['id'],))
            conn.commit()
            return False, 'code_expired'

        if row['attempts'] >= EMAIL_MFA_MAX_ATTEMPTS:
            cur.execute('UPDATE email_mfa_codes SET used = 1 WHERE id = ?', (row['id'],))
            conn.commit()
            return False, 'too_many_attempts'

        cur.execute(
            'UPDATE email_mfa_codes SET attempts = attempts + 1 WHERE id = ?',
            (row['id'],)
        )

        # Constant-time comparison
        if hmac.compare_digest(code.strip(), row['code']):
            cur.execute('UPDATE email_mfa_codes SET used = 1 WHERE id = ?', (row['id'],))
            conn.commit()
            return True, None
        else:
            conn.commit()
            return False, 'invalid_code'
    except Exception:
        conn.rollback()
        logging.exception('Error verifying MFA code')
        return False, 'internal_error'
    finally:
        conn.close()


def send_mfa_email(email, code):
    """Send the MFA verification email."""
    subject = 'Your Company verification code'
    body = (
        f'Your verification code is: {code}\n\n'
        f'This code expires in 10 minutes.\n'
        f'If you didn\'t request this, ignore this email.'
    )
    return send_email(email, subject, body)


# =============================================
# Email MFA endpoints
# =============================================

@app.route('/email-mfa/status', methods=['POST', 'OPTIONS'])
def email_mfa_status():
    if request.method == 'OPTIONS':
        return '', 204
    session_token = request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user:
        return jsonify({'error': 'invalid session'}), 401
    conn = get_conn()
    row = conn.execute('SELECT enabled FROM email_mfa WHERE user_id = ?', (user['user_id'],)).fetchone()
    conn.close()
    enabled = bool(row and row['enabled']) if row else False
    return jsonify({'enabled': enabled})


@app.route('/email-mfa/enable', methods=['POST', 'OPTIONS'])
def email_mfa_enable():
    if request.method == 'OPTIONS':
        return '', 204
    session_token = request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user or not user.get('email'):
        return jsonify({'error': 'invalid session or no email'}), 401
    user_id = user['user_id']

    if not check_rate_limit(user_id):
        return jsonify({'error': 'rate_limit', 'message': 'Too many code requests. Try again later.'}), 429

    code = generate_mfa_code()
    store_mfa_code(user_id, code)
    if not send_mfa_email(user['email'], code):
        return jsonify({'error': 'email_failed', 'message': 'Failed to send email'}), 500

    return jsonify({'status': 'code_sent'})


@app.route('/email-mfa/enable/verify', methods=['POST', 'OPTIONS'])
def email_mfa_enable_verify():
    if request.method == 'OPTIONS':
        return '', 204
    session_token = request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user:
        return jsonify({'error': 'invalid session'}), 401

    body = request.get_json(force=True, silent=True) or {}
    code = body.get('code', '')
    if not code:
        return jsonify({'error': 'missing code'}), 400

    success, err = verify_mfa_code(user['user_id'], code)
    if not success:
        return jsonify({'status': 'invalid', 'error': err}), 400

    # Enable email MFA for this user
    conn = get_conn()
    conn.execute(
        'INSERT OR REPLACE INTO email_mfa (user_id, enabled, created_at) VALUES (?, 1, ?)',
        (user['user_id'], now_iso())
    )
    conn.commit()
    conn.close()
    return jsonify({'status': 'enabled'})


@app.route('/email-mfa/disable', methods=['POST', 'OPTIONS'])
def email_mfa_disable():
    if request.method == 'OPTIONS':
        return '', 204
    session_token = request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user:
        return jsonify({'error': 'invalid session'}), 401

    # Disable email MFA
    conn = get_conn()
    conn.execute('DELETE FROM email_mfa WHERE user_id = ?', (user['user_id'],))
    conn.commit()
    conn.close()
    return jsonify({'status': 'disabled'})


@app.route('/email-mfa/challenge', methods=['POST', 'OPTIONS'])
def email_mfa_challenge():
    if request.method == 'OPTIONS':
        return '', 204
    body = request.get_json(force=True, silent=True) or {}
    session_token = body.get('session_token') or request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user or not user.get('email'):
        return jsonify({'error': 'invalid session or no email'}), 401
    user_id = user['user_id']

    # Check if email MFA is enabled for this user
    conn = get_conn()
    row = conn.execute('SELECT enabled FROM email_mfa WHERE user_id = ?', (user_id,)).fetchone()
    conn.close()
    if not row or not row['enabled']:
        return jsonify({'error': 'email_mfa_not_enabled'}), 400

    if not check_rate_limit(user_id):
        return jsonify({'error': 'rate_limit', 'message': 'Too many code requests. Try again later.'}), 429

    code = generate_mfa_code()
    store_mfa_code(user_id, code)
    if not send_mfa_email(user['email'], code):
        return jsonify({'error': 'email_failed', 'message': 'Failed to send email'}), 500

    return jsonify({'status': 'code_sent'})


@app.route('/email-mfa/challenge/verify', methods=['POST', 'OPTIONS'])
def email_mfa_challenge_verify():
    if request.method == 'OPTIONS':
        return '', 204
    body = request.get_json(force=True, silent=True) or {}
    session_token = body.get('session_token') or request.headers.get('X-Session-Token')
    if not session_token:
        return jsonify({'error': 'missing session token'}), 401
    user = verify_session(session_token)
    if not user:
        return jsonify({'error': 'invalid session'}), 401

    code = body.get('code', '')
    if not code:
        return jsonify({'error': 'missing code'}), 400

    success, err = verify_mfa_code(user['user_id'], code)
    if success:
        return jsonify({'status': 'verified'})
    else:
        return jsonify({'status': 'invalid', 'error': err}), 400


if __name__ == '__main__':
    host = os.environ.get('SIGNER_BIND_ADDR', '0.0.0.0')
    port = int(os.environ.get('SIGNER_PORT', '8080'))
    print('Starting mock signer on %s:%s (data_dir=%s)' % (host, port, BASE_DIR), file=sys.stderr)
    # Initialize DB and attempt migration from JSON stores
    init_db()
    migrate_json_to_sqlite()
    app.run(host=host, port=port)
