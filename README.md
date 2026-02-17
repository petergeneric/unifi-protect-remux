# Overview

This is a free, open source tool that converts Ubiquiti's proprietary .ubv files into standard .MP4 files with H.264/HEVC video and AAC audio. The conversion is a "remux": no transcoding takes place, the exact same video and audio essence are simply copied into a different, industry standard container; because of this the process is reasonably fast and not CPU intensive even on a low spec ARM machine.


# Getting Started

Use the following instructions to get the unifi-protect-remux tool working:

1. Go to the [github releases page](https://github.com/petergeneric/unifi-protect-remux/releases) and download the latest remux x86_64 binary (get the binary appropriate to your system - if you want to run on Ubiquiti hardware you'll need to use the linux `aarch64-legacy.tar.gz` file)
2. Upload this to your server and extract with ```tar -xaf unifi-protect-remux-*.tar.gz```
3. Transfer .ubv files from your NVR to your x86 server (on cloudkey v2, .ubv files are found under /srv/unifi-protect/video).
4. Run: ```./remux *.ubv```
5. By default, both audio and video will be extracted. If you do not want audio, add "--with-audio=false" to your command

## Paid assistance available

I've been in the video software field for over 20 years, and have assisted with footage recovery and analysis (including production of custom review and reporting interfaces) for a case that became a **high-profile UK Public Inquiry**, and involved **hundreds of thousands of hours of highly sensitive footage**, so I understand the complex environment my users often work within.

While a technical user with UNIX experience should be able to use my tools to extract footage even from corrupted files with relative ease by themselves, if you don't have a suitable person available or simply want somebody with experience for complex cases you can hire me to assist privately with footage extraction, analysis, or report writing.

If you think you need paid assistance, please reach out first by [creating a GitHub issue](https://github.com/petergeneric/unifi-protect-remux/issues/new) to arrange a private discussion (N.B. do not include any confidential information in the Issue text).


## Source Media: Live Systems

This tool is designed to work on `_0_rotating_` .ubv files. You can get a list of these files with the following on your Unifi Protect system:

```
find /srv/unifi-protect/video -type f -name "*_0_rotating_*.ubv"
```

or

```
find /volume1/.srv/unifi-protect/video -type f -name "*_0_rotating_*.ubv"
```

## Source Media: Damaged Systems

If your Ubiquiti NVR has been damaged, you should be able to mount it on any Linux system (or other OS supporting `ext4`) using a USB to SATA (or USB to NVMe) adapter. I highly recommend mounting as read-only.

**If your footage is sensitive or likely to involve a court case,** I highly recommend involving a qualified data forensics company to extract the files for you since they understand how to handle devices in a safe way while preserving and documenting chain of custody, and be able to attest to the integrity of any files they recover. Retain the `ubv` files they retrieve from the disks: these are the originals, my `remux` tool can always derive standard MP4s from them, but you cannot produce the original from an MP4.

## Command-line arguments

Run `./remux --help` for advanced use. This will produce output like so:

```
Usage of remux:
  --with-audio
    	If true, extract audio (default true)
  --with-video
    	If true, extract video (default true)
  --mp4
    	If true, will create an MP4 as output (default true)
        Note: MP4 output currently requires `--with-video=true`; audio-only MP4 is not supported
  --fast-start
    	If true, generated MP4 files will have faststart enabled for better streaming. Increases remux IO cost (default false)
  --output-folder string
    	The path to output remuxed files to. "SRC-FOLDER" to put alongside .ubv files (default "./")
  --version
    	Display version and quit
  --force-rate int
    	If non-zero, forces CFR at the defined rate
```


# UBV Format

As part of my reverse engineering work, I am [documenting the UBV format](https://github.com/petergeneric/unifi-protect-remux/wiki).

# Build from source

See COMPILING.md for more detail


# Windows Version

I provide a Windows build of this tool; I have not been able to test this in a Windows environment, but it should work. Please reach out with feedback if you do use it on Windows.
