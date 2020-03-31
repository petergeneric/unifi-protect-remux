#!/bin/bash

NATIVE_CMD=./Remux

if [ [ -z "$1" ] -or [ "$1" = "--help" ] ] ; then
	echo "Usage: $0 [file1] [file2] [etc]" >&2
	exit 2
fi


while [ -n "$1" ]
do
	# Try to use the native binary where possible, falling back on the platform JVM
	if [ -e "$NATIVE_CMD" ] ; then
		$NATIVE_CMD $ubv
	else
		echo "Native command $NATIVE_CMD unavailable, falling back on .jar executed with your native Java VM"
		java -cp target/*.jar Remux $ubv
	fi
	
	ubv="$1"
	shift

	if [ "$?" = "0" ] ; then
		ffmpeg -i "${ubv}.h264" -vcodec copy -y "${ubv}.mp4"
		
		rm "${ubv}.h264"
	else
		echo "Extraction of primitive stream failed for ${ubv}." >&2
		exit 1
	fi
done