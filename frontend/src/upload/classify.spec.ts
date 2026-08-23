import { describe, expect, it } from 'vitest'

import { categorize, classifyFiles } from './classify'

function file(name: string): File {
  return new File([new Blob(['x'])], name)
}

describe('categorize', () => {
  it.each(['DSC01.jpg', 'DSC02.JPEG', 'DSC03.jpe', 'DSC04.png', 'DSC05.tif', 'DSC06.tiff', 'DSC07.webp', 'DSC08.heic', 'DSC09.heif'])(
    'treats %s as an image',
    (name) => {
      expect(categorize(name)).toBe('image')
    }
  )

  it.each(['clip.mp4', 'clip.MOV', 'clip.m4v'])('treats %s as a video', (name) => {
    expect(categorize(name)).toBe('video')
  })

  it.each([
    'DSC01.arw', 'DSC02.sr2', 'DSC03.srf', 'DSC04.cr2', 'DSC05.cr3', 'DSC06.crw', 'DSC07.nef',
    'DSC08.nrw', 'DSC09.raf', 'DSC10.orf', 'DSC11.rw2', 'DSC12.raw', 'DSC13.pef', 'DSC14.srw',
    'DSC15.x3f', 'DSC16.3fr', 'DSC17.iiq', 'DSC18.mos', 'DSC19.mef', 'DSC20.erf', 'DSC21.kdc',
    'DSC22.dcr', 'DSC23.mrw', 'DSC24.rwl', 'DSC25.fff'
  ])('treats %s as RAW', (name) => {
    expect(categorize(name)).toBe('raw')
  })

  it('treats .dng as RAW, not as an image — it is a RAW container (§4)', () => {
    expect(categorize('DSC26.dng')).toBe('raw')
    expect(categorize('DSC26.DNG')).toBe('raw')
  })

  it('treats anything else as unsupported', () => {
    expect(categorize('notes.txt')).toBe('unsupported')
    expect(categorize('archive.zip')).toBe('unsupported')
    expect(categorize('no-extension')).toBe('unsupported')
  })
})

describe('classifyFiles', () => {
  it('never rejects the whole batch for the presence of RAW or unsupported files (§4)', () => {
    const result = classifyFiles([file('a.jpg'), file('b.arw'), file('c.txt'), file('d.mp4')])
    expect(result.accepted.map((f) => f.name)).toEqual(['a.jpg', 'd.mp4'])
    expect(result.rejectedRaw.map((f) => f.name)).toEqual(['b.arw'])
    expect(result.rejectedUnsupported.map((f) => f.name)).toEqual(['c.txt'])
  })

  it('an all-accepted batch has empty rejection lists', () => {
    const result = classifyFiles([file('a.jpg'), file('b.mp4')])
    expect(result.rejectedRaw).toHaveLength(0)
    expect(result.rejectedUnsupported).toHaveLength(0)
  })
})
