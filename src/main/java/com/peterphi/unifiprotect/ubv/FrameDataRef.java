package com.peterphi.unifiprotect.ubv;

public class FrameDataRef
{
	public final UbvTrack track;
	public final int offset;
	public final int size;

	public long wc;
	public int tbc;

	public FrameDataRef(final UbvTrack track, final int offset, final int size)
	{
		this.track = track;
		this.offset = offset;
		this.size = size;
	}
}
