# Tutorial: Full-Stack Development Environment with PDN

This tutorial walks you through setting up a complete full-stack development environment using PDN (PRoot Distro Next) on Termux/Android.

## Prerequisites

- Termux installed on Android
- PDN installed (`curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install.sh | bash`)
- Basic familiarity with Linux command line

## Overview

We'll set up:
- **Node.js 20** - Application runtime
- **PostgreSQL 16** - Primary database
- **Redis 7** - Caching & sessions
- **PDN Service Supervisor** - Auto-start & management

## Step 1: Install Containers

```bash
# Install base images
podnext install ubuntu:24.04

# Create containers for each service
podnext create nodejs --image ubuntu:24.04
podnext create postgres --image ubuntu:24.04
podnext create redis --image ubuntu:24.04
```

## Step 2: Configure Services

Create service configurations using the example files:

```bash
# Copy example configs
mkdir -p ~/.config/pn-next/services
cp examples/services/*.toml ~/.config/pn-next/services/

# Edit configs for your environment
nano ~/.config/pn-next/services/nodejs.toml
nano ~/.config/pn-next/services/postgres.toml
nano ~/.config/pn-next/services/redis.toml
```

## Step 3: Start Service Supervisor

```bash
# Start the daemon
podnext-daemon start

# Load services
podnext service load nodejs
podnext service load postgres
podnext service load redis

# Start all services
podnext service start nodejs postgres redis
```

## Step 4: Verify Services

```bash
# Check status
podnext service status

# Check logs
podnext service logs postgres

# Test connections
podnext exec postgres -- psql -U postgres -c "SELECT version();"
podnext exec redis -- redis-cli ping
```

## Step 5: Auto-Start on Boot

```bash
# Enable auto-start
podnext boot enable nodejs postgres redis

# Check boot status
podnext boot status
```

## Step 6: Run Your Application

```bash
# Enter Node.js container
podnext login nodejs

# Inside container:
npm init -y
npm install express pg ioredis

# Create your app...
```

## Service Management Commands

```bash
# View all services
podnext service list

# Restart a service
podnext service restart postgres

# View resource usage
podnext quota usage postgres

# Check health
podnext service status redis

# Stop all services
podnext service stop nodejs postgres redis
```

## Troubleshooting

### Service won't start
```bash
# Check logs
podnext service logs <service-name>

# Check health
podnext service status <service-name>

# Restart with fresh state
podnext service stop <service-name>
podnext service start <service-name>
```

### Port conflicts
Check `podnext service status` for port mappings and adjust in service config.

### Resource limits
Adjust in service config:
```toml
[resources]
cpu_limit = 2.0
memory_limit = "4GB"
```

## Next Steps

- Set up GitHub Actions runner in a container
- Configure monitoring with Prometheus/Grafana
- Set up backup/restore for databases
- Explore PDN's D-Bus IPC for inter-service communication

## Resources

- [PDN Documentation](https://github.com/michaelamoetoevsky/PDN)
- [Service Supervisor Reference](https://github.com/michaelamoetoevsky/PDN/tree/main/crates/pn-next/src/supervisor.rs)
- [Example Configurations](../examples/services/)
- [Community Discussions](https://github.com/michaelamoetoevsky/PDN/discussions)

---

*Happy coding with PDN! 🚀*