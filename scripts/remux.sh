#!/bin/bash

# This is a helper script intended to make the process of running a remux on an x86 system easier.
# It can be used directly if you have ubnt_ubvinfo that works on your platform,
# or used after a "prepare.sh" invocation on a CloudKey has run the tool and written the output
# to a .txt file beside the .ubv
#
#

# Try to pick an appropriate pre-compiled version if available.
# This avoids the need for the user to have Maven and Java available locally
if [ -e "./remux"] ; then
	NATIVE_CMD=./remux
elif [ $(uname -i) = "aarch64" ] ; then
	NATIVE_CMD=./remux-arm64
else
	NATIVE_CMD=./remux-amd64
fi

function helptext() {
	echo "Usage: $0 [file1] [file2] [etc]"
	echo ""
	echo "Runs an extraction locally, rewrapping all the resulting .h264 files into .mp4 using FFmpeg"
}

function die_with() {
	echo "$*" >&2
	exit 1
}

if [ -z "$1" ] ; then
	helptext
	exit 1
elif [ "$1" = "--help" ] ; then
	helptext
	exit 0
fi

while [ -n "$1" ]
do
	ubv="$1"
	shift
	
	ubv_base=$(basename "$ubv" ".ubv")

	# Try to use the native binary where possible, falling back on the platform Java if necessary
	if [ -e "$NATIVE_CMD" ] ; then
		$NATIVE_CMD $ubv
	elif [ -e "remux.jar" ] ; then
		java -cp remux.jar Remux $ubv
	else
		echo "Native command $NATIVE_CMD unavailable, as is pre-compiled .jar so trying to build jar with maven..."
		
		JAR_FILE=target/remux-1.0-SNAPSHOT.jar
		
		if [ ! -e "$JAR_FILE" ] ; then
			mvn package || die_with "Maven compile failed! Please check you have Maven installed and confirm the output of the compile operation"
		fi
		
		java -cp $JAR_FILE Remux $ubv
	fi
	

	if [ "$?" = "0" ] ; then
		for f in ${ubv_base}*.h264
		do
			OUTPUT_FILE_BASE=$(basename "$f" ".h264")
			ffmpeg -i "$f" -vcodec copy -y "${OUTPUT_FILE_BASE}.mp4"
			
			rm $f
		done
	else
		echo "Extraction of primitive stream failed for ${ubv}." >&2
		exit 1
	fi
done
