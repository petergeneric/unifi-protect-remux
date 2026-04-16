UBV Remux - Windows CLI tools
=============================

This archive contains:

  remux.exe          Convert .ubv files to MP4
  ubv-info.exe       Inspect .ubv file structure
  ubv-anonymise.exe  Zero out payload data in .ubv files
  *.dll              FFmpeg runtime libraries (keep alongside the .exe files)
  vc_redist.x64.exe  Microsoft Visual C++ 2015-2022 Redistributable

First-time setup
----------------

Before running any of the tools, install the Visual C++ Runtime:

  1. Double-click vc_redist.x64.exe
  2. Accept the prompts (admin rights required)

This only needs to be done once per machine. If the runtime is already
installed the installer will tell you so and exit without making changes.

If you skip this step, the tools will fail to start with an error about
a missing MSVCP140.dll or VCRUNTIME140.dll.

If you installed UBV Remux via the GUI installer, you can skip this step
- the installer already did it for you.

Usage
-----

Extract the archive to a folder of your choice and run the tools from a
Command Prompt or PowerShell window in that folder. For example:

  remux.exe path\to\recording.ubv

Pass --help to any of the tools for full usage information.

More info: https://github.com/peterwright/unifi-protect-remux
