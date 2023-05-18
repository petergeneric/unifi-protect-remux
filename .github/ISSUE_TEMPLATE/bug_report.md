---
name: Bug report
about: Create a report to help us improve
title: ''
labels: ''
assignees: petergeneric

---

**Describe the bug**
A clear and concise description of what the bug is.
Please check the following common mistakes:
1. Make sure that you're using a "_0_rotating_" file as a source, since these have the original footage and are what this tool is primarily designed to decode.
2. Make sure that the ubnt_ubvinfo command can be accessed and works (e.g. ```ubnt_ubvinfo | head -n1``` should output ```At least one file path is mandatory```)
3. Make sure you're using the latest version of the tool - see https://github.com/petergeneric/unifi-protect-remux/releases

**Command line arguments to reproduce**
Please provide the full commandline you're using; note that .ubv filenames do not contain confidential information. They do contain the MAC address of your camera, so if you're concerned about that please remove that section of the filename only. The filename structure is: "macaddress_0_rotating_timestamp.ubv"

**Output**
Please provide the full output of the remux command

**ubnt_ubvinfo output**
Please attach a copy of the output of the following command
```ubnt_ubvinfo -P -f your_problem_file.ubv > output.txt```

This output can be quite large, so you may want to compress it before uploading.

The output of this command contains no confidential information - it includes position data on the video and audio frames within the .ubv file, and date+timestamps of each frame.

**Your Hardware**
 - OS you're running the command on: [e.g. Ubuntu, WSL1, WSL2, macOS, UnifiOS]

**Additional context**
Add any other context about the problem here.
