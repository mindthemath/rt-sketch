# git-crypt: Quick Reference

## What it does

git-crypt encrypts specific files in a git repo using AES-256. Encrypted files look like binary garbage to anyone without the key. Diffs, logs, and editing work normally for key holders.

## Setup (first time, one repo)

### 1. Install

```bash
# macOS
brew install git-crypt

# Debian/Ubuntu
apt install git-crypt
```

### 2. Initialize

```bash
cd your-repo
git-crypt init
```

This generates a unique symmetric key stored in `.git/git-crypt/keys/default`.

### 3. Configure which files to encrypt

Add patterns to `.gitattributes`:

```
# encrypt the drawing module
src/drawing/** filter=git-crypt diff=git-crypt

# also the debug binary that exercises it
src/bin/draw_curves.rs filter=git-crypt diff=git-crypt
```

```bash
git add .gitattributes
git commit -m "configure git-crypt"
```

Any files matching those patterns will be encrypted on push and decrypted on checkout.

### 4. Export the key

```bash
mkdir -p ~/git-crypt-keys
git-crypt export-key ~/git-crypt-keys/your-repo.key
```

The key is a small binary file (~136 bytes). One key per repo. Store it somewhere safe — a password manager (as a file attachment) or an encrypted drive.

## Decrypting on another machine

### 1. Get the key file onto the machine

```bash
# option: scp from another machine
scp user@source-machine:~/git-crypt-keys/your-repo.key ~/git-crypt-keys/your-repo.key

# option: download from password manager
# (save the file attachment to ~/git-crypt-keys/your-repo.key)
```

### 2. Clone and unlock

```bash
git clone git@github.com:you/your-repo.git
cd your-repo
git-crypt unlock ~/git-crypt-keys/your-repo.key
```

Unlock is a one-time operation per clone. After this, everything works normally — no extra steps on pull, push, or checkout.

## Verifying it works

```bash
# check encryption status of tracked files
git-crypt status

# shows something like:
#     encrypted: secrets/api.key
#     encrypted: src/private/module.rs
# not encrypted: README.md
# not encrypted: src/lib.rs
```

## Key facts

- The key is **raw bytes**, not text — don't try to copy-paste it
- Each `git-crypt init` generates a **unique key** — keys are not shared across repos
- The key has **no identity** attached — anyone with the file can decrypt
- `git-crypt lock` re-encrypts files in your working tree (useful before handing off a machine)
- SSH keys and git-crypt keys are independent: SSH controls repo access, git-crypt controls file readability

## Managing keys for multiple repos

```
~/git-crypt-keys/
├── rt-sketch.key
├── my-other-project.key
└── work-repo.key
```

Transfer the whole directory to a new machine:

```bash
scp -r ~/git-crypt-keys/ user@new-machine:~/git-crypt-keys/
```

Then unlock each repo as needed.

For storing as plaintext:

```bash
# encode for storage/transfer
base64 < rt-sketch.key > rt-sketch.key.b64

# decode on the other end
base64 -d < rt-sketch.key.b64 > rt-sketch.key
git-crypt unlock rt-sketch.key
```

## better storage:
```bash
# Store it
git-crypt export-key /dev/stdout | base64 | pass insert -m git-crypt/rt-sketch

# Restore on another machine
pass git-crypt/rt-sketch | base64 -d | git-crypt unlock -
```
