package com.peterphi.unifiprotect.util;

import java.io.File;
import java.io.IOException;

abstract class AbstractCommand
{
	private Boolean available;
	private String binary;


	protected abstract String[] getDefaultCommandExistsCmdline();

	protected abstract File[] getStaticLocations();


	protected String getDefaultCommand()
	{
		return getDefaultCommandExistsCmdline()[0];
	}


	public boolean isInstalled()
	{
		if (available == null)
		{
			available = getBinary() != null;
		}

		return available;
	}


	protected String getBinary()
	{
		if (binary == null)
		{
			// Test if binary is available on PATH
			try
			{
				if (spawnNoOutput(getDefaultCommandExistsCmdline()) == 0)
					binary = getDefaultCommand();
			}
			catch (Exception e)
			{
				// invocation failed, binary probably not on PATH
			}

			// Fallback to testing known paths
			// This is awful and hacky, but makes things easier when running on a Cloud Key by a user who hasn't modified the PATH
			for (File staticLocation : getStaticLocations())
			{
				if (staticLocation.exists() && staticLocation.isFile() && staticLocation.canExecute())
				{
					binary = staticLocation.getAbsolutePath();
					break;
				}
			}
		}

		return binary;
	}


	protected static int spawnNoOutput(final String... cmd) throws IOException, InterruptedException
	{
		final ProcessBuilder pb = new ProcessBuilder(cmd);

		pb.redirectError(ProcessBuilder.Redirect.DISCARD);
		pb.redirectOutput(ProcessBuilder.Redirect.DISCARD);
		Process process = pb.start();

		return process.waitFor();
	}
}
