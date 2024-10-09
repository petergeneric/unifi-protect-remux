![Go](https://github.com/petergeneric/unifi-protect-remux/workflows/Go/badge.svg)

Overview
========

This is a tool that converts Ubiquiti's proprietary .ubv files into standard .MP4 files with H.264 and AAC. The conversion is a remux: no transcoding takes place, the exact same video and audio essence are placed into a different container; because of this the process is reasonably fast and not CPU intensive even on a low spec ARM machine.

Native binaries are available. The easiest way (but worst performing) is to follow the "Quick Start For Ubiquiti Hardware" section. More advanced users can follow the instructions for x86 using qemu.

Please look in the github "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases for builds of the latest version.

Latest Features
---------------
This release supports audio, and if FFmpeg is available then it will create MP4 files. Currently it will create one MP4 for each partition of a multi-partition .ubv, naming the files based on the date+time the video starts.

Audio muxing is new; it should account for audio/video synchronisation, however this has not been extensively tested (I don't have any camera samples where AV sync is particularly obvious). If you're experiencing issues and can supply a .ubv for me to examine please raise an issue and get in touch.


QUICK START: FOR UBIQUITI HARDWARE
==================================
Instructions for Cloud Key Gen 2 Plus (and other Ubiquiti hardware). Due to the relatively slow CPU and IO subsystems, running a remux this way will be somewhat slow. See below for instructions on how to run under Linux x86 below.

1. Go to the "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases and download the latest remux ARM64 binary (N.B. "ARM64", not "x86")
2. Upload this to your Cloud Key home folder using SSH (SCP) with ```tar -xf remux-arm64.tar.gz && rm remux-arm64.tar.gz && chmod +x remux```
3. Download the latest FFmpeg ARM Static Release (available on the releases page, and also from https://johnvansickle.com/ffmpeg/)
4. Upload this to your Cloud Key and extract it with ```xz -d ffmpeg-release-arm64-static.tar.xz && tar -xf ffmpeg-release-arm64-static.tar && mv ffmpeg*arm64-static/ffmpeg ./ && rm ffmpeg-release-arm64-static.tar.xz && chmod +x ffmpeg```
5. Run the following on your cloudkey: ```export PATH=$HOME:$PATH``` (you'll either need to run this every time you log in or put it in your ```.bashrc``` file)
6. Navigate to where your .ubv video is located (base path: /srv/unifi-protect/video).
7. Run: ```remux *.ubv```
8. By default, audio and video is extracted. If you do not need to extract audio, add "--with-audio=false" to your command

If FFmpeg is not installed (or if the command fails) the remux tool will leave the raw .aac and .h264 bitstream files; these can be combined with a variety of tools. 


QUICK START: FOR x86 LINUX
==========================

Dependencies: ubnt_ubvinfo from CloudKey
----------------------------------------
N.B. If you have files from a (discontinued, unsupported) x86 Protect installation, you can use the native x86 ```ubnt_ubvinfo``` tool from it - just copy that file to ```/usr/bin/ubnt_ubvinfo```. Otherwise, the following instructions will let you run the ARM binary on your x86 machine at a slight performance penalty using QEMU.
(These instructions are Ubuntu/Debian specific; pull requests welcome for CentOS/Arch equivalent)

1. On your x86 machine, install qemu-user with: ```apt install -y qemu-user gcc-aarch64-linux-gnu```
2. Copy ```/usr/share/unifi-protect/app/node_modules/.bin/ubnt_ubvinfo``` from your CloudKey to ```/usr/bin/arm-ubnt_ubvinfo``` on your x86 machine
3. Run the following:
```
sudo tee /usr/bin/ubnt_ubvinfo <<EOF
#!/bin/sh
export QEMU_LD_PREFIX=/usr/aarch64-linux-gnu
exec qemu-aarch64 /usr/bin/arm-ubnt_ubvinfo "\$@"
EOF
chmod +x /usr/bin/ubnt_ubvinfo
```

Dependencies: FFmpeg
------------------------
(These instructions are Ubuntu/Debian specific; pull requests welcome for CentOS/Arch equivalent)

To install FFmpeg, run:
```
apt install -y ffmpeg
```

Extracting video
----------------
Once the dependencies are installed, use the following instructions to get the unifi-protect-remux tool working:

1. Go to the "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases and download the latest remux x86_64 binary
2. Upload this to your Linux server and extract with ```tar -zxf remux-x86_64.tar.gz```
3. Transfer .ubv files from your CloudKey to your x86 server (on cloudkey, .ubv files are found under /srv/unifi-protect/video).
4. Run: ```remux *.ubv```
5. By default, audio and video is extracted. If you do not need to extract audio, add "--with-audio=false" to your command

If FFmpeg is not installed (or if the command fails) the remux tool will leave the raw .aac and .h264 bitstream files; these can be combined with a variety of tools. 


Command-line arguments
======================

```
Usage of remux:
  -with-audio
    	If true, extract audio
  -with-video
    	If true, extract video (default true)
  -mp4
    	If true, will create an MP4 as output (default true)
  -output-folder string
    	The path to output remuxed files to. "SRC-FOLDER" to put alongside .ubv files (default "./")
  -version
    	Display version and quit
  -force-rate int
    	If non-zero, adds a -r argument to FFmpeg invocations
```

NOTE ON x86 WITHOUT QEMU
=======================

The quickstart instructions above show how to run the AARCH64 ubnt_ubvinfo tool shipped with Unifi Protect on your x86 hardware. This relies on qemu-user. If this tool is not available on your x86 machine (and you don't have the native x86 version of ubnt_ubvinfo -- for copyright reasons I can't supply this tool) then you will need to run the ubnt_ubvinfo command on your Ubiquiti hardware, then transfer the .ubv files along with a cached output of ubnt_ubvinfo to your x86 machine for final extraction.

See the scripts folder in the repository: these scripts generate a summary .txt file that you can pull back alongside the .ubv and run ```remux``` on your x86 machine.



FINDING SOURCE MEDIA
====================

This tool works on .ubv files but is primarily designed to work on "_0_rotating_" .ubv files. You can get a list of these files with the following on your Unifi Protect system:

```
find /srv/unifi-protect/video -type f -name "*_0_rotating_*.ubv"
```

RUNNING
=======

Entirely locally
----------------

If you have a version of ```ubnt_ubvinfo``` and FFmpeg that runs on your system (and it's on your PATH), you can simply run ```remux somefile.ubv```

Run ubvinfo remotely and remux locally
--------------------------------------

For better performance (and lower IO and memory overhead on your Unifi Protect server) you can use ubnt_ubvinfo on your Unifi Protect system, writing the output to a .txt file on disk.
A helper script is provided for this:

1. Simply copy ```prepare.sh``` to your Unifi Protect system and then run ```prepare.sh *.ubv```
2. Next, transfer the .ubv and the .ubv.txt file(s) back to your main system.
3. Finally, run the remux binary locally on the .ubv file; the tool will automatically find and use the .ubv.txt file prepared on your Protect system.


BUILD FROM SOURCE
=================

Simply run "make package" to compile


DEPENDENCIES
============

FFmpeg
------
This tool extracts bare audio/video bitstreams, and relies on FFmpeg to mux them into an MP4.

You can install FFmpeg on Ubuntu with:

```
apt install ffmpeg
```

There are also several FFmpeg static builds available, see https://ffmpeg.org/download.html
