# Security Policy

## Supported Versions

We provide security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you find a security vulnerability in PDN, please report it responsibly.

### How to Report

**Please do NOT create a public GitHub issue for security vulnerabilities.**

Instead, please report security vulnerabilities by email to **security@pdn.example.com**

Include the following information:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Affected versions
- Any suggested fixes that you know

### What to Expect

1. **Acknowledgment**: We will acknowledge receipt within 48 hours
2. **Assessment**: We will assess the vulnerability and determine severity
3. **Timeline**: We will provide an estimated timeline for a fix
4. **Coordination**: We will coordinate with you on the disclosure timeline
5. **Credit**: We will credit you in the security advisory if you want that credit

## Security Best Practices for Users

### Container Security

- Always use the latest PDN version
- Keep container images updated
- Use minimal base images
- Run containers with least privileges
- Use resource quotas to limit container resources

### Network Security

- Use the built-in network isolation of PDN
- Avoid exposure of unnecessary ports
- Use TLS for inter-service communication
- Configure firewall rules on the host

### Container Images

- Verify image signatures when possible
- Use trusted registries
- Scan images for vulnerabilities
- Rebuild images regularly

### Service Supervisor

- Use resource quotas to prevent resource exhaustion
- Configure health checks for all services
- Set appropriate restart policies
- Monitor service logs regularly

## Known Security Considerations

### PRoot Limitations

PDN uses PRoot for containerization. PRoot has known limitations:

- PRoot uses `ptrace` for syscall interception
- This is not full kernel-level isolation like KVM or gVisor
- PRoot does not fully intercept some syscalls
- PRoot is not suitable for untrusted code execution

### Termux Environment

- PDN runs on Android without root
- The Android security model limits PDN
- SELinux policies can restrict some operations
- File system permissions differ from standard Linux

## Vulnerability Disclosure Timeline

| Phase | Timeline |
|-------|----------|
| Initial Response | 48 hours |
| Triage | 7 days |
| Fix Development | 30-90 days (depending on severity) |
| Release | Within 7 days of fix |
| Public Disclosure | After fix released + 7 days |

## Security Advisories

Security advisories will be published as:

- GitHub Security Advisories
- Release notes with security fixes
- Email notifications to subscribers

## Contact

For security-related questions or concerns:

- Email: security@pdn.example.com
- PGP Key: Available on request

---

*This policy is based on industry best practices. The PDN team updates this policy periodically.*
