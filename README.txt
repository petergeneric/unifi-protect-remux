OVERVIEW
========

This is a Java tool that extracts an H.264 video bitstream from Ubiquiti's proprietary Unifi Protect .ubv container.


LIMITATIONS
===========

1. Currently this tool only works with single partition files. In the future it will be expanded to work with multiple partitions, by creating multiple .mp4 files
2. Currently only the video stream is extracted. The underlying can be used to extract the audio bitstream too, but currently the tool does not do this. The resulting AAC and H.264 bitstreams can be muxed together into an MP4 trivially.
3. The tool expects a UNIX system (tested on Linux and MacOS, BSD should work too). The code will run just fine under Windows, but the provided scripts expect UNIX.

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

https://www.graalvm.org/docs/reference-manual/native-image/

N.B. for Ubuntu you'll also need the following packages:
apt install build-essential libz-dev

Then simply run "make native-image" to invoke Maven and then run the resulting .jar through Graal

Install FFmpeg
--------------

This tool extracts a bare H.264 bitstream from the .ubv files, and relies on FFmpeg to mux this into an MP4 file.

You can install this on Ubuntu with:

apt install ffmpeg

RUNNING
=======

Entirely locally
----------------

If you have a version of ubnt_ubvinfo that runs on your system (and it's on your PATH), you can run "remux.sh some.ubv"

Run ubvinfo remotely and remux locally
--------------------------------------

You can use ubnt_ubvinfo on your Unifi Protect system, writing the output to a .txt file on disk.
A helper script is provided for this:

1. Simply copy "prepare.sh" to your Unifi Protect system and then run "prepare.sh some.ubv".
2. Next, transfer both the .ubv and the .ubv.txt file back to your main system.
3. Finally, run remux.sh locally on the .ubv file; the tool will automatically find and use the .ubv.txt file prepared on your Protect system.

