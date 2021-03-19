extern "C" {
struct CompressResult {
	const char* buf;
	long length;
	int retcode;
};
void deallocBuf(char* buf);
CompressResult compress(char* _inData, long inLength);
}
