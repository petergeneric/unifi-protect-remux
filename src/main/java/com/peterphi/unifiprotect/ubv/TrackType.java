package com.peterphi.unifiprotect.ubv;

public enum TrackType
{
	AUDIO,
	VIDEO;


	public String getDefaultExtension()
	{
		switch (this)
		{
			case AUDIO:
				return ".aac";
			case VIDEO:
				return ".h264";
			default:
				throw new RuntimeException("Unexpected com.peterphi.unifiprotect.ubv.TrackType! " + this);
		}
	}


	public boolean isDefaultTrack(final int trackId)
	{
		return (this == VIDEO && trackId == 7) || (this == AUDIO && trackId == 1000);
	}
}
