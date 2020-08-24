Overview
========

This is a tool that converts Ubiquiti's proprietary .ubv files into standard .MP4 files with H.264 and AAC. The conversion is a remux: no transcoding takes place, the exact same video and audio essence are placed into a different container; because of this the process is reasonably fast and not CPU intensive even on a low spec ARM machine.

Native binaries are available. Most people should use ARM64 and run this on their Ubiquiti hardware.

Please look in the github "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases for builds of the latest version.

Latest Features
---------------
This release supports audio, and if FFmpeg is available then it will create MP4 files. Currently it will create one MP4 for each partition of a multi-partition .ubv, naming the files based on the date+time the video starts.

Audio muxing is new; it should account for audio/video synchronisation, however this has not been extensively tested (I don't have any camera samples where AV sync is particularly obvious). If you're experiencing issues and can supply a .ubv for me to examine please raise an issue and get in touch.

QUICK START: FOR UBIQUITI HARDWARE
=================================
Instructions for Cloud Key Gen 2 Plus (and other Ubiquiti hardware):

1. Go to the "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases and download the latest remux-arm64 binary (N.B. "ARM64", not "AMD64")
2. Upload this to your Cloud Key home folder using SSH (SCP) and extract with ```tar -xf remux-arm64.tar.gz && rm remux-arm64.tar.gz && chmod +x remux-arm64```
3. Download the latest FFmpeg ARM Static Release (available on the releases page, and also from https://johnvansickle.com/ffmpeg/)
4. Upload this to your Cloud Key and extract it with ```xz -d ffmpeg-release-arm64-static.tar.xz && tar -xf ffmpeg-release-arm64-static.tar && mv ffmpeg*arm64-static/ffmpeg ./ && rm ffmpeg-release-arm64-static.tar.xz && chmod +x ffmpeg```
5. Run the following on your cloudkey: ```export PATH=$HOME:$PATH``` (you'll either need to run this every time you log in or put it in your .bashrc file)
6. Navigate to where your .ubv video is located (base path: /srv/unifi-protect/video).
7. Run: ```remux *.ubv```
8. By default, only video is extracted. If you need to extract audio too, add "--with-audio" to your command

If FFmpeg is not installed (or if the command fails) the remux tool will leave the raw .aac and .h264 bitstream files; these can be combined with a variety of tools. 

NOTE ON AMD64 VERSION
================================

If you have the old (unsupported) x86 version of Protect, extract the ubnt_ubvinfo tool from that (see the path referenced in the scripts/ folder of this repo) and put it in your PATH and you can use the AMD64 binary. This will be significantly faster than running on a Cloud Key. For copyright reasons I can't supply this tool.

If you don't have the x86 Protect installer, you can run some key tasks on your Ubiquiti gear (see the scripts folder). These scripts generate a summary .txt file that you can pull back alongside the .ubv and run remux-amd64 on your x86 machine.


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
