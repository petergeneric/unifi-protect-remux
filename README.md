Overview
========

This is a tool that converts Ubiquiti's proprietary .ubv files into standard .MP4 files with H.264/HEVC video and AAC audio. The conversion is a remux: no transcoding takes place, the exact same video and audio essence are placed into a different container; because of this the process is reasonably fast and not CPU intensive even on a low spec ARM machine.

Please look in the github "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases for builds of the latest version.


Dependencies: FFmpeg
------------------------
(These instructions are Ubuntu/Debian specific; pull requests welcome for CentOS/Arch/Windows equivalent)

To install FFmpeg, run:
```
apt install -y ffmpeg
```

Extracting video
----------------
Once the dependencies are installed, use the following instructions to get the unifi-protect-remux tool working:

1. Go to the "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases and download the latest remux x86_64 binary (use the binary appropriate to your system)
2. Upload this to your Linux server and extract with ```tar -zxf remux-x86_64.tar.gz```
3. Transfer .ubv files from your CloudKey to your x86 server (on cloudkey, .ubv files are found under /srv/unifi-protect/video).
4. Run: ```remux *.ubv```
5. By default, audio and video is extracted. If you do not need to extract audio, add "--with-audio=false" to your command

If FFmpeg is not installed (or if the command fails) the remux tool will leave the raw .aac and .h264 bitstream files; these can be combined with a variety of tools. 


Command-line arguments
======================

```
Usage of remux:
  --with-audio
    	If true, extract audio (default true)
  --with-video
    	If true, extract video (default true)
  --mp4
    	If true, will create an MP4 as output (default true)
  --fast-start
    	If true, generated MP4 files will have faststart enabled for better streaming. Increases remux IO cost (default false)
  --output-folder string
    	The path to output remuxed files to. "SRC-FOLDER" to put alongside .ubv files (default "./")
  --version
    	Display version and quit
  --force-rate int
    	If non-zero, forces CFR at the defined rate
```

FINDING SOURCE MEDIA
====================

This tool works on .ubv files but is primarily designed to work on "_0_rotating_" .ubv files. You can get a list of these files with the following on your Unifi Protect system:

```
find /srv/unifi-protect/video -type f -name "*_0_rotating_*.ubv"
```


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
