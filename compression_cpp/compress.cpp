#include <string>
#include <vector>
#include <algorithm>
#include "compress.h"

using namespace std;

/*struct CompressResult {
	const char* buf;
	long length;
	int retcode;
};*/

void deallocBuf(char* buf) {
	delete[] buf;
}

int getOccurLength(char* newPtr, int newLength, char* oldPtr, int oldLength, int& disp, int minDisp = 1)
{
    disp = 0;

    if (newLength == 0)
        return 0;

    int maxLength = 0;

    for (int i = 0; i < oldLength - minDisp; i++)
    {
        char* currentOldStart = oldPtr + i;
        int currentLength = 0;

        for (int j = 0; j < newLength; j++)
        {
            if (*(currentOldStart + j) != *(newPtr + j))
                break;
            currentLength++;
        }

        if (currentLength > maxLength)
        {
            maxLength = currentLength;
            disp = oldLength - i;

            if (maxLength == newLength)
                break;
        }
    }

    return maxLength;
}

CompressResult compress(char* _inData, long inLength)
{
	CompressResult res;
    if (inLength > 0xFFFFFF) {
    	res.buf = _inData;
    	res.length = inLength;
    	res.retcode = -2;
        return res;
    }

    //ifstream _inStream(inFile, ios::binary | ios::in);
    //ofstream _outStream(outFile, ios::binary | ios::out);

    //char* _inData = new char[inLength];
    //_inStream.read(_inData, inLength);

    vector<char> _outData;

    /*if (!_inStream)
        return -1;*/

    _outData.push_back(0x11);
    _outData.push_back((char)(inLength & 0xFF));
    _outData.push_back((char)((inLength >> 0x08) & 0xFF));
    _outData.push_back((char)((inLength >> 0x0F) & 0xFF));

    int _compLength = 4;
    char* _inStart = &_inData[0];

    char* _outBuffer = new char[8 * 4 + 1];
    _outBuffer[0] = 0;

    int _bufLength = 1, _bufBlocks = 0;
    int _charRead = 0;

    while (_charRead < inLength)
    {
        if (_bufBlocks == 8)
        {
            for (int i = 0; i < _bufLength; i++)
                _outData.push_back(_outBuffer[i]);

            _compLength += _bufLength;

            _outBuffer[0] = 0;
            _bufLength = 1;
            _bufBlocks = 0;
        }

        int _disp;
        int _oldLength = min(_charRead, 0x1000);
        int _newLength = getOccurLength(_inStart + _charRead, min((int)(inLength - _charRead), 0x10110), _inStart + _charRead - _oldLength, _oldLength, _disp);

        if (_newLength < 3)
            _outBuffer[_bufLength++] = *(_inStart + (_charRead++));

        else
        {
            _charRead += _newLength;
            _outBuffer[0] |= (char)(1 << (7 - _bufBlocks));

            if (_newLength > 0x110)
            {
                _outBuffer[_bufLength] = 0x10;
                _outBuffer[_bufLength] |= (char)(((_newLength - 0x111) >> 12) & 0x0F);
                _bufLength++;

                _outBuffer[_bufLength] = (char)(((_newLength - 0x111) >> 4) & 0xFF);
                _bufLength++;

                _outBuffer[_bufLength] = (char)(((_newLength - 0x111) << 4) & 0xF0);
            }

            else if (_newLength > 0x10)
            {
                _outBuffer[_bufLength] = 0x00;
                _outBuffer[_bufLength] |= (char)(((_newLength - 0x111) >> 4) & 0x0F);
                _bufLength++;

                _outBuffer[_bufLength] = (char)(((_newLength - 0x111) << 4) & 0xF0);
            }

            else
                _outBuffer[_bufLength] = (char)(((_newLength - 1) << 4) & 0xF0);

            _outBuffer[_bufLength] |= (char)(((_disp - 1) >> 8) & 0x0F);
            _bufLength++;

            _outBuffer[_bufLength] = (char)((_disp - 1) & 0xFF);
            _bufLength++;
        }

        _bufBlocks++;
    }

    if (_bufBlocks > 0)
    {
        for (int i = 0; i < _bufLength; i++)
            _outData.push_back(_outBuffer[i]);

        _compLength += _bufLength;
    }
    res.buf = (const char*)_outData.data();
    res.length = _outData.size();
    res.retcode = 0;
    return res;
}
