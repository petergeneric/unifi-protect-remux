#!/bin/bash

# This script is designed to prepare a .txt file with .ubv structure info on a CloudKey Gen2
# to allow the execution of remux.sh on another machine (e.g. with more IO and with FFmpeg available)
#

if [ -z "$UBVINFO" ] ; then
	UBVINFO=/usr/share/unifi-protect/app/node_modules/.bin/ubnt_ubvinfo
fi

function helptext() {
	echo "Usage: $0 *_0_rotating_*.ubv"
}

if [ -z "$1" ] ; then
	helptext

	exit 1
elif [ "$1" == "--help" ] ; then
	helptext

	exit 0
fi


while [ -n "$1" ]
do
	$UBVINFO -P -f "$1" > "$1.txt"
	if [ "$?" != "0" ] ; then
		echo "ubnt_ubvinfo invocation failed!" >&2
		exit 1
	fi
	
	shift
done
