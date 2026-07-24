import { MarkdownIt } from '@gsgfs/markdown-it-rs-wasm/full'
import sample from './sample.md?raw'
import './index.css'

const parser = new MarkdownIt()
const source = document.querySelector('#source')
const preview = document.querySelector('#result-preview')
const htmlOutput = document.querySelector('#result-html')

source.value = sample

const render = () => {
    const html = parser.render(source.value)
    preview.innerHTML = html
    htmlOutput.textContent = html
}

let renderTimeout
source.addEventListener('input', () => {
    clearTimeout(renderTimeout)
    renderTimeout = setTimeout(render, 100)
})

const tabs = document.querySelectorAll('#tabs-toggle a.nav-link')
const contents = document.querySelectorAll('.tab-content')

const activateTab = (tab) => {
    tabs.forEach((item) => item.classList.remove('active'))
    tab.classList.add('active')

    contents.forEach((item) => {
        item.style.display = 'none'
    })
    document.querySelector(`#${tab.dataset.toggle}`).style.display = 'block'
}

tabs.forEach((tab) => {
    tab.addEventListener('click', (event) => {
        event.preventDefault()
        activateTab(tab)
    })
})

render()
activateTab(tabs[0])
