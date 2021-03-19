extern "C" {
	struct CompressResult {
		const char* buf;
		long length;
		int retcode;
	};
	CompressResult compress(char* _inData, long inLength, char* (*bufCreator)(long));
}
