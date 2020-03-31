#!/bin/bash

UBVINFO=/usr/share/unifi-protect/app/node_modules/.bin/ubnt_ubvinfo

if [ [ -z "$1" ] -or [ "$1" = "--help" ] ] ; then
	echo "Usage: $0 *_0_rotating_*.ubv" >&2
	exit 2
fi


while [ -n "$1" ]
do
	$UBVINFO -t 7 -P -f "$1"
	if [ "$?" != "0" ] ; then
		echo "ubnt_ubvinfo invocation failed!" >&2
		exit 1
	fi
	
	shift
done