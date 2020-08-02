OVERVIEW
========

This is a Java tool that extracts an H.264 video bitstream from Ubiquiti's proprietary Unifi Protect .ubv container. Native binaries are available for ARM64 and AMD64 Linux systems.

QUICK START FOR CLOUD KEY GEN2
==============================

1. Go to the "releases" page at https://github.com/petergeneric/unifi-protect-remux/releases and download the latest remux-arm64 binary.
2. Upload this to your Cloud Key, leaving in the home folder
3. Navigate to where your .ubv video is located (/srv/unifi-protect/video)
4. Run ~/remux-amd64 with a single argument, the .ubv file you want to extract from (e.g. ~/remux-amd64 2020/08/01/XXXXXXXXXXXX_0_rotating_1596300058863.ubv)
5. Once the tool completes, you'll find a series of .h264 files in the same directory as the .ubv input
6. Transfer these to your machine. The .h264 files are raw video bitstreams. It will be easier if you remux them into an .MP4 wrapper using FFmpeg
7. Install FFmpeg for your platform (https://ffmpeg.zeranoe.com/builds/ for Windows, apt install ffmpeg for Linux, brew install ffmpeg for MacOS using HomeBrew or https://evermeet.cx/ffmpeg/ otherwise)
8. Use the following command to remux each .h264 into an MP4: ffmpeg -i example.h264 -vcodec copy example.mp4


LIMITATIONS
===========

1. Currently only the video stream is extracted. The underlying can be used to extract the audio bitstream too, but currently the tool does not do this. Raise an issue if there's interest in this functionality.
2. The tool expects a UNIX system (tested on Linux and MacOS, BSD should work too). The code will run just fine under Windows, it's just untested.

FINDING SOURCE MEDIA
====================

This tool works on .ubv files but is primarily designed to work on "_0_rotating_" .ubv files. You can get a list of these files with the following on your Unifi Protect system:

find /srv/unifi-protect/video -type f -name "*_0_rotating_*.ubv"

BUILD FROM SOURCE
=================

Simply run "mvn package" to produce a .jar that can be executed. See remux.sh 

Build Native Binary
-------------------

You can build a native binary using GraalVM's native-image tool. Instructions on how to install the dependencies are at:

https://www.graalvm.org/docs/reference-manual/native-image/ (the installation is as simple as: unpack latest graal CE, run "gu install native-image")

N.B. for Ubuntu you'll also need the following packages (N.B. libz-dev may also be zlib1g-dev):
apt install build-essential libz-dev

Make graal your default JVM (export JAVA_HOME=/path/to/graal), put it on the path (export PATH=$PATH:/path/to/graal/bin)

Then simply run "make native-image", which will run a Maven build and generate a "remux" binary via Graal

Install FFmpeg
--------------

This tool extracts a bare H.264 bitstream from the .ubv files, and relies on FFmpeg to mux this into an MP4 file.

You can install this on Ubuntu with:

apt install ffmpeg

RUNNING
=======

Entirely locally
----------------

If you have a version of ubnt_ubvinfo that runs on your system (and it's on your PATH), you can simply run "remux somefile.ubv"

Run ubvinfo remotely and remux locally
--------------------------------------

You can use ubnt_ubvinfo on your Unifi Protect system, writing the output to a .txt file on disk.
A helper script is provided for this:

1. Simply copy "prepare.sh" to your Unifi Protect system and then run "prepare.sh some.ubv".
2. Next, transfer both the .ubv and the .ubv.txt file back to your main system.
3. Finally, run remux.sh locally on the .ubv file; the tool will automatically find and use the .ubv.txt file prepared on your Protect system.

