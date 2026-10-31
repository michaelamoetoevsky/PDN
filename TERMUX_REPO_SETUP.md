# PDN Self-Hosted Termux Repository

Since termux-packages requires established community adoption (PR #33069 closed), use this self-hosted approach.

## Repository Setup

The fork already exists at: **https://github.com/michaelamoetoevsky/termux-packages**

### Option 1: GitHub Pages (Recommended)

1. **Enable GitHub Pages** on the fork:
   - Go to https://github.com/michaelamoetoevsky/termux-packages/settings/pages
   - Source: "Deploy from a branch" → `gh-pages` branch → `/ (root)`
   - This creates: `https://michaelamoetoevsky.github.io/termux-packages/`

2. **Build and publish packages** using GitHub Actions:
   ```yaml
   # .github/workflows/build.yml (add to fork)
   name: Build PDN Package
   on:
     push:
       branches: [master]
   jobs:
     build:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - name: Build pdn
           run: |
             ./build-package.sh pdn
         - name: Deploy to Pages
           uses: peaceiris/actions-gh-pages@v3
           if: github.ref == 'refs/heads/master'
           with:
             github_token: ${{ secrets.GITHUB_TOKEN }}
             publish_dir: ./packages
   ```

### Option 2: Direct Package Distribution (Simpler)

**Just use the built .deb from GitHub Actions artifacts:**

1. Run build locally or in CI:
   ```bash
   cd ~/workspace/termux-packages-shallow
   ./build-package.sh pdn
   ```

2. Upload `.deb` to PDN GitHub Releases alongside binaries

3. Users install directly:
   ```bash
   # Download and install
   wget https://github.com/michaelamoetoevsky/PDN/releases/download/v0.1.0/pdn_0.1.0_aarch64.deb
   dpkg -i pdn_0.1.0_aarch64.deb
   apt install -f  # fix dependencies
   ```

### Option 3: Custom Termux Repository (Best UX)

**Create a simple repo index:**

```bash
# On the fork, create a repo index
mkdir -p repo
cp packages/pdn/*.deb repo/
dpkg-scanpackages repo /dev/null | gzip -9c > repo/Packages.gz
```

**Users add your repo:**
```bash
# Add to $PREFIX/etc/apt/sources.list.d/pdn.list
echo "deb https://michaelamoetoevsky.github.io/termux-packages/repo/ ./" > $PREFIX/etc/apt/sources.list.d/pdn.list

# Update and install
apt update
apt install pdn
```

---

## Current Package Status

| Item | Status |
|------|--------|
| Recipe (`packages/pdn/build.sh`) | ✅ Complete |
| SHA256 verified | ✅ `8bc0bb931cfb07b7e11aba92ec20f8250e0c4ff2ba0af665d0ce14863e58b63e` |
| Dependencies declared | ✅ proot, proot-distro |
| Build deps declared | ✅ rust, clang, lld, cmake, ninja, protobuf, etc. |
| Binaries install | ✅ podnext, podnext-daemon, pn-gateway |
| Completions | ✅ bash, zsh, fish |
| Termux:Boot | ✅ Auto-start script |
| Config template | ✅ supervisor.toml |

---

## Next Steps

1. **Immediate**: Add PDN .deb to GitHub Releases v0.1.0
2. **Short-term**: Set up GitHub Pages on fork for custom repo
3. **Long-term**: Build community adoption, then resubmit to termux-packages

## Files Ready
- `termux-package/recipe/build.sh` - Complete build script
- `termux-package/SUBMISSION_GUIDE.md` - Documentation
- `~/workspace/termux-packages-shallow/packages/pdn/build.sh` - In fork