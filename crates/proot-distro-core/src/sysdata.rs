//! System data management for PRoot Distro

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Sysdata manager for fake system data
pub struct SysdataManager {
    rootfs: PathBuf,
}

impl SysdataManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            rootfs: PathBuf::from("/"),
        })
    }

    pub fn with_rootfs(rootfs: PathBuf) -> Result<Self> {
        Ok(Self { rootfs })
    }

    /// Setup fake system data (proc, sys, etc.)
    pub fn setup_fake_sysdata(&self, rootfs: &Path) -> Result<()> {
        // Create /proc files
        self.create_proc_files(rootfs)?;

        // Create /sys files
        self.create_sys_files(rootfs)?;

        // Create /dev files
        self.create_dev_files(rootfs)?;

        Ok(())
    }

    fn create_proc_files(&self, rootfs: &Path) -> Result<()> {
        let proc_dir = PathBuf::from("/proc");
        let proc_path = rootfs.join(&proc_dir);
        fs::create_dir_all(&proc_path)?;

        // /proc/version
        fs::write(
            proc_path.join("version"),
            "Linux version 5.15.0 (android) #1 SMP PREEMPT\n",
        )?;

        // /proc/cpuinfo
        fs::write(
                proc_path.join("cpuinfo"),
                "processor\t: 0\nBogoMIPS\t: 38.40\nFeatures\t: fp asimd evtstrm crc32 cpuid\nCPU implementer\t: 0x41\nCPU architecture\t: 8\nCPU variant\t: 0x0\nCPU part\t: 0xd03\nCPU revision\t: 4\n\nprocessor\t: 1\nBogoMIPS\t: 38.40\nFeatures\t: fp asimd evtstrm crc32 cpuid\nCPU implementer\t: 0x41\nCPU architecture\t: 8\nCPU variant\t: 0x0\nCPU part\t: 0xd03\nCPU revision\t: 4\n\nprocessor\t: 2\nBogoMIPS\t: 38.40\nFeatures\t: fp asimd evtstrm crc32 cpuid\nCPU implementer\t: 0x41\nCPU architecture\t: 8\nCPU variant\t: 0x0\nCPU part\t: 0xd03\nCPU revision\t: 4\n\nprocessor\t: 3\nBogoMIPS\t: 38.40\nFeatures\t: fp asimd evtstrm crc32 cpuid\nCPU implementer\t: 0x41\nCPU architecture\t: 8\nCPU variant\t: 0x0\nCPU part\t: 0xd03\nCPU revision\t: 4\n"
            )?;

        // /proc/meminfo
        fs::write(
                proc_path.join("meminfo"),
                "MemTotal:        4046852 kB\nMemFree:         1024000 kB\nMemAvailable:   2500000 kB\nBuffers:           10000 kB\nCached:          2000000 kB\nSwapCached:            0 kB\nActive:          1500000 kB\nInactive:        1500000 kB\nActive(anon):     500000 kB\nInactive(anon):   500000 kB\nActive(file):    1000000 kB\nInactive(file):  1000000 kB\nUnevictable:           0 kB\nMlocked:               0 kB\nSwapTotal:       1048576 kB\nSwapFree:        1048576 kB\nDirty:                100 kB\nWriteback:             0 kB\nAnonPages:         500000 kB\nMapped:            200000 kB\nShmem:             100000 kB\nSlab:              100000 kB\nSReclaimable:       50000 kB\nSUnreclaim:         50000 kB\nKernelStack:        10000 kB\nPageTables:         10000 kB\nNFS_Unstable:          0 kB\nBounce:                0 kB\nWritebackTmp:          0 kB\nCommitLimit:     3072000 kB\nCommitted_AS:    1500000 kB\nVmallocTotal:   251658176 kB\nVmallocUsed:           0 kB\nVmallocChunk:          0 kB\nHardwareCorrupted:     0 kB\nAnonHugePages:         0 kB\nShmemHugePages:        0 kB\nShmemPmdMapped:        0 kB\nFileHugePages:         0 kB\nFilePmdMapped:         0 kB\nCmaTotal:              0 kB\nCmaFree:               0 kB\nHugePages_Total:       0\nHugePages_Free:        0\nHugePages_Rsvd:        0\nHugePages_Surp:        0\nHugePagesize:       2048 kB\nHugetlb:               0 kB\nDirectMap4k:       100000 kB\nDirectMap2M:     2000000 kB\nDirectMap1G:           0 kB\n"
            )?;

        // /proc/cmdline
        fs::write(
            proc_path.join("cmdline"),
            "console=tty0 console=ttyS0,115200 root=/dev/mmcblk0p2 rootwait\n",
        )?;

        // /proc/uptime
        fs::write(proc_path.join("uptime"), "1000.00 2000.00\n")?;

        // /proc/stat
        fs::write(
                proc_path.join("stat"),
                "cpu  1000 200 300 10000 400 0 500 0 0 0\ncpu0 250 50 75 2500 100 0 125 0 0 0\ncpu1 250 50 75 2500 100 0 125 0 0 0\ncpu2 250 50 75 2500 100 0 125 0 0 0\ncpu3 250 50 75 2500 100 0 125 0 0 0\nintr 1000000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\nctxt 100000\nbtime 1600000000\nprocesses 100\nprocs_running 2\nprocs_blocked 1\n"
            )?;

        Ok(())
    }

    fn create_sys_files(&self, rootfs: &Path) -> Result<()> {
        // Create /sys/class
        let class_dir = rootfs.join("sys/class");
        fs::create_dir_all(class_dir.join("net"))?;
        fs::create_dir_all(class_dir.join("block"))?;
        fs::create_dir_all(class_dir.join("tty"))?;
        fs::create_dir_all(class_dir.join("graphics"))?;

        // /sys/devices
        let devices_dir = rootfs.join("sys/devices");
        fs::create_dir_all(devices_dir.join("platform"))?;
        fs::create_dir_all(devices_dir.join("virtual"))?;

        // /sys/kernel
        fs::create_dir_all(rootfs.join("sys/kernel"))?;

        Ok(())
    }

    fn create_dev_files(&self, rootfs: &Path) -> Result<()> {
        // Create basic device nodes
        let dev_dir = rootfs.join("dev");
        fs::create_dir_all(&dev_dir)?;

        // Use mknod for device nodes (would need CAP_MKNOD)
        // For now, just create regular files as placeholders

        Ok(())
    }
}
