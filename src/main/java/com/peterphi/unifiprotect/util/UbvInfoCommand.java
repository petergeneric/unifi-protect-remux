package com.peterphi.unifiprotect.util;

import com.peterphi.unifiprotect.ubv.UbvInfoParser;
import com.peterphi.unifiprotect.ubv.UbvPartition;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.List;

public class UbvInfoCommand extends AbstractCommand
{
	/**
	 * Static locations we fall back on if ubnt_ubvinfo isn't available on PATH
	 */
	private static final File[] STATIC_LOCATIONS = new File[]{new File(
			"/usr/share/unifi-protect/app/node_modules/.bin/ubnt_ubvinfo")};


	@Override
	protected String[] getDefaultCommandExistsCmdline()
	{
		return new String[]{"ubnt_ubvinfo", "-h"};
	}


	@Override
	protected File[] getStaticLocations()
	{
		return STATIC_LOCATIONS;
	}


	public List<UbvPartition> ubvinfo(final File inputFile, final boolean videoOnly) throws IOException, InterruptedException
	{
		if (!isInstalled())
			throw new RuntimeException("Cannot invoke ubnt_ubvinfo: does not appear to be installed on the local system!");

		// Write ubvinfo to a temporary file
		File tempFile = null;
		try
		{
			tempFile = File.createTempFile("ubv", ".txt");

			// Special-case video only extraction (since output data volume will be MUCH lower)
			final ProcessBuilder pb;
			if (videoOnly)
				pb = new ProcessBuilder(getBinary(), "-t", "7", "-P", "-f", inputFile.getAbsolutePath());
			else
				pb = new ProcessBuilder(getBinary(), "-P", "-f", inputFile.getAbsolutePath());

			pb.redirectError(ProcessBuilder.Redirect.DISCARD);
			pb.redirectOutput(tempFile);
			Process process = pb.start();

			final int exitCode = process.waitFor();

			if (exitCode != 0)
				throw new IllegalArgumentException("ubvinfo failed with code " + exitCode);

			return UbvInfoParser.parse(Files.lines(tempFile.toPath()));
		}
		finally
		{
			if (tempFile != null)
				tempFile.delete();
		}
	}
}
